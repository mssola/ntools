use crate::errors::{ContextError, ContextErrorReason};
use crate::mapping::Mapping;
use crate::node::{ControlType, NodeType, PNode, PString};
use crate::opcodes::CONTROL_FUNCTIONS;
use std::collections::HashMap;

/// The name of the global context as used internally.
const GLOBAL_CONTEXT: &str = "Global";

/// A Bundle represents a set of bytes that can be encoded as binary data.
#[derive(Debug, Default, Clone, Eq, Ord, PartialEq, PartialOrd)]
pub struct Bundle {
    /// The bytes which make up any encodable element for the application. The
    /// capacity is of three bytes maximum, but the actual size is encoded in
    /// the `size` property.
    pub bytes: [u8; 3],

    /// The amount of bytes which have actually been set on this bundle.
    pub size: u8,

    /// The address where the given bytes are to be placed on the resulting
    /// binary file.
    pub address: usize,

    /// If this bundle encodes an instruction, the amount of cycles it takes for
    /// the CPU to actually execute it.
    pub cycles: u8,

    /// Whether the cost in cycles is affected when crossing a page boundary.
    pub affected_on_page: bool,

    /// Whether the bytes on `bytes` contain the final value or not. This is
    /// used for internal purposes only.
    pub resolved: bool,
}

impl Bundle {
    /// Create a default bundle but with the given `resolved` status.
    pub fn new(resolved: bool) -> Self {
        Self {
            resolved,
            ..Default::default()
        }
    }

    /// Create a bundle tailored for filling purposes.
    pub fn fill(value: u8) -> Self {
        Self {
            bytes: [value, 0, 0],
            size: 1,
            address: 0,
            cycles: 0,
            affected_on_page: false,
            resolved: true,
        }
    }
}

/// The type of object being referenced, which is either a value as-is, or an
/// address that needs to be interpreted when fetching it.
#[derive(Debug, Clone)]
pub enum ObjectType {
    Address,
    Value,
}

/// Bundle and metadata which is stored on the context table for a given
/// variable or label.
#[derive(Debug, Clone)]
pub struct Object {
    /// Bundle representing the actual value.
    pub bundle: Bundle,

    /// The mapping index where the object was found. Note that this index
    /// doesn't mean much on the table, but it has to mean something by the
    /// caller.
    pub mapping: usize,

    /// The segment index within the referenced mapping where the object was
    /// found. Note that this index doesn't mean much on the table, but it has
    /// to mean something by the caller.
    pub segment: usize,

    /// The type for this object.
    pub object_type: ObjectType,
}

impl Object {
    /// Create a default bundle with the given metadata parameters.
    pub fn new(mapping: usize, segment: usize, object_type: ObjectType) -> Self {
        Self {
            bundle: Bundle::default(),
            mapping,
            segment,
            object_type,
        }
    }
}

/// Context holds information about the different scopes being defined, the
/// current scope, and has a map of all the variables defined for each scope.
#[derive(Debug)]
pub struct Context {
    /// Tracks the contexts that we have entered at any given point. The last
    /// element is the actual context, and it will be empty if we are in the
    /// global context.
    stack: Vec<String>,

    /// Map of objects for any given context. The key is the name of the
    /// context, and the value is another map. This inner map has the variable
    /// name as the key, and the Bundle as a value.
    pub map: HashMap<String, HashMap<String, Object>>,

    /// Map of labels for any given context. Note that this only keeps track of
    /// the amount of labels that have been defined, which might include
    /// anonymous positions. This is primarily used on relative addressing where
    /// the name of the label might not be provided (e.g. anonymous relative
    /// reference).
    pub labels: HashMap<String, Vec<Object>>,
}

impl Default for Context {
    fn default() -> Self {
        Self::new()
    }
}

impl Context {
    /// Returns a new empty context.
    pub fn new() -> Self {
        Context {
            stack: vec![],
            map: HashMap::from([(String::from(GLOBAL_CONTEXT), HashMap::new())]),
            labels: HashMap::from([(String::from(GLOBAL_CONTEXT), vec![])]),
        }
    }

    /// Returns the value of the object represented by the given `id`. Note that
    /// this `id` can be scoped or not, and this function will try to pick the
    /// variable from the right scope. The value itself will be resolved if the
    /// type is ObjectType::Address.
    pub fn get_variable(&self, id: &PString, mappings: &[Mapping]) -> Result<Object, ContextError> {
        // First of all, figure out the name of the scope and the real name of
        // the variable. If this was not scoped at all (None case when trying to
        // rsplit by the "::" operator), then we assume it's a global variable.
        let (scope_name, var_name) = match id.value.rsplit_once("::") {
            Some((scope, name)) => (scope, name),
            None => (self.name(), id.value.as_str()),
        };

        // And with that, the only thing left is to find the scope and the
        // variable in it.
        match self.map.get(scope_name) {
            Some(scope) => match scope.get(var_name) {
                Some(var) => match var.object_type {
                    ObjectType::Value => Ok(var.clone()),
                    ObjectType::Address => Ok(self.resolve_label(mappings, var)?),
                },
                None => Err(ContextError {
                    message: format!(
                        "could not find variable '{}' in {}",
                        var_name,
                        self.to_human_with(scope_name)
                    ),
                    line: id.line,
                    reason: ContextErrorReason::UnknownVariable,
                    global: false,
                }),
            },
            None => Err(ContextError {
                message: format!("did not find scope '{}'", scope_name),
                line: id.line,
                reason: ContextErrorReason::BadScope,
                global: false,
            }),
        }
    }

    /// Given an `object` which is located via `mappings`, resolve the effective
    /// address.
    ///
    /// NOTE: this function asserts that the given `object` is of type
    /// ObjectType::Address, otherwise it doesn't make sense to call it.
    pub fn resolve_label(
        &self,
        mappings: &[Mapping],
        object: &Object,
    ) -> Result<Object, ContextError> {
        assert!(matches!(object.object_type, ObjectType::Address));

        let mut ret = object.clone();

        let mapping = &mappings[ret.mapping];
        let internal_offset = u16::from_le_bytes([ret.bundle.bytes[0], ret.bundle.bytes[1]]);
        let segment_offset = crate::mapping::segment_offset(mapping, ret.segment);
        let addr = mapping.start as usize + segment_offset as usize + internal_offset as usize;

        // Avoid weird out of bound references for addresses.
        if addr > u16::MAX as usize {
            return Err(ContextError {
                line: 0,
                message: format!("address {:x} is out of bounds", addr),
                reason: ContextErrorReason::Bounds,
                global: true,
            });
        }

        let addr_bytes = (addr as u16).to_le_bytes();

        ret.bundle.bytes[0] = addr_bytes[0];
        ret.bundle.bytes[1] = addr_bytes[1];

        Ok(ret)
    }

    /// Sets a value for an object identified by `id`. If `overwrite` is set to
    /// true, then this value will be set even if the id already existed,
    /// otherwise it will return a ContextError
    pub fn set_variable(
        &mut self,
        id: &PString,
        object: &Object,
        overwrite: bool,
    ) -> Result<(), ContextError> {
        let scope_name = self.name().to_string();
        let scope = self.map.get_mut(&scope_name).unwrap();

        match scope.get_mut(&id.value) {
            Some(sc) => {
                if !overwrite {
                    return Err(ContextError {
                        message: format!(
                            "'{}' already defined in {}: you cannot re-assign names",
                            id.value,
                            self.to_human()
                        ),
                        line: id.line,
                        reason: ContextErrorReason::Redefinition,
                        global: false,
                    });
                }
                *sc = object.clone();
            }
            None => {
                scope.insert(id.value.clone(), object.to_owned());
            }
        }

        Ok(())
    }

    /// Add a new label to the list of known labels.
    pub fn add_label(&mut self, object: &Object) {
        let scope_name = self.name().to_string();
        let scope = self.labels.get_mut(&scope_name).unwrap();

        scope.push(object.clone());
    }

    /// Change the current context given a `node`. Returns a tuple which states:
    ///   0. Whether the context has changed.
    ///   1. Whether a caller can bundle nodes safely.
    pub fn change_context(&mut self, node: &PNode) -> Result<(bool, bool), ContextError> {
        // The parser already guarantees that the control node is
        // from a function that we already know, so calling `unwrap`
        // is not dangerous.
        let control = CONTROL_FUNCTIONS
            .get(&node.value.value.to_lowercase())
            .unwrap();

        // If the control function does not touch the context, leave early.
        if !control.touches_context {
            return Ok((false, true));
        }

        // And push/pop the context depending on the control being used.
        match node.node_type {
            NodeType::Control(ControlType::StartMacro) => {
                self.context_push(&node.left.clone().unwrap());
                Ok((true, false))
            }
            NodeType::Control(ControlType::StartProc)
            | NodeType::Control(ControlType::StartScope) => {
                self.context_push(&node.left.clone().unwrap());
                Ok((true, true))
            }
            NodeType::Control(ControlType::EndMacro)
            | NodeType::Control(ControlType::EndProc)
            | NodeType::Control(ControlType::EndScope) => {
                self.context_pop(&node.value)?;
                Ok((true, true))
            }
            _ => Ok((false, true)),
        }
    }

    /// Change the current context to the given one identified by `name`,
    /// disregarding any check. This is to be used when switching a context to
    /// set a very specific value for that context. You should call
    /// `force_context_pop` immediately.
    pub fn force_context_switch(&mut self, name: &String) {
        self.stack.push(name.to_owned());
    }

    /// Remove the last context being used if any. In contrast with
    /// `context_pop`, this one does not error out, but does nothing in case we
    /// are in the global context. This is to be used in conjunction with
    /// `force_context_switch`.
    pub fn force_context_pop(&mut self) {
        if !self.stack.is_empty() {
            self.stack.truncate(self.stack.len() - 1);
        }
    }

    /// Returns the amount of labels that have been submitted so far for the
    /// current scope.
    pub fn labels_seen(&self) -> usize {
        let scope_name = self.name().to_string();

        match self.labels.get(&scope_name) {
            Some(labels) => labels.len(),
            None => 0,
        }
    }

    /// Returns the bundle which is representative for the label being
    /// referenced in a relative way. The relation is given on the `rel`
    /// parameter, with a value between -4 or +4 where negative values represent
    /// previous labels and positive values next ones (e.g. -2 means "2 labels
    /// before"). This is in relation to `labels_seen`, which states how many
    /// labels have been seen by the caller at this point.
    pub fn get_relative_label(
        &self,
        rel: isize,
        labels_seen: usize,
        mappings: &[Mapping],
    ) -> Result<Object, ContextError> {
        // Bound check: the given 'rel' parameter has a proper value.
        assert!(
            rel < 5 && rel > -5 && rel != 0,
            "bad parameter for relative label"
        );

        // Bound check: you cannot reference a past label that doesn't exist.
        // This is the programmer's to blame, not on us, so don't assert.
        if labels_seen == 0 && rel < 0 {
            return Err(ContextError {
                line: 0,
                message: "cannot reference an unknown previous label".to_string(),
                reason: ContextErrorReason::Label,
                global: false,
            });
        }

        // Get the labels as referenced in the current context, and also the
        // index that we will be using.
        let scope_name = self.name().to_string();
        let labels = self.labels.get(&scope_name).unwrap();
        let idx = if rel > 0 {
            labels_seen as isize + rel - 1
        } else {
            labels_seen as isize + rel
        };

        // Bound check: is the programmer referencing an "out of bounds" label?
        // If so then it's a mistake on their part.
        if idx < 0 || idx >= labels.len() as isize {
            return Err(ContextError {
                line: 0,
                message: "cannot reference bogus label (out of bounds)".to_string(),
                reason: ContextErrorReason::Label,
                global: false,
            });
        }

        // Everything should be fine from here on, simply return the bundle that
        // was being referenced.
        self.resolve_label(mappings, &labels[idx as usize])
    }

    // Pushes a new context given a `node`, which holds the identifier of the
    // new scope.
    fn context_push(&mut self, id: &PNode) {
        let name = match self.stack.last() {
            Some(n) => format!("{}::{}", n, id.value.value),
            None => id.value.value.clone(),
        };

        // Actually push the name to the stack and initialize it on the variable
        // map.
        self.stack.push(name.clone());
        self.map.entry(name.clone()).or_default();
        self.labels.entry(name).or_default();
    }

    // Pops out the latest context that was pushed.
    fn context_pop(&mut self, id: &PString) -> Result<(), ContextError> {
        if self.stack.is_empty() {
            return Err(ContextError {
                message: format!("missplaced '{}' statement", id.value),
                reason: ContextErrorReason::BadScope,
                line: id.line,
                global: false,
            });
        }

        self.stack.truncate(self.stack.len() - 1);
        Ok(())
    }

    /// Returns the name of the current context.
    pub fn name(&self) -> &str {
        match self.stack.last() {
            Some(name) => name,
            None => GLOBAL_CONTEXT,
        }
    }

    /// Returns true if we are in the global scope.
    pub fn is_global(&self) -> bool {
        self.stack.is_empty()
    }

    // Returns a human-readable string representing the current context.
    fn to_human(&self) -> String {
        match self.stack.last() {
            Some(n) => format!("'{}'", n),
            None => "the global scope".to_string(),
        }
    }

    // Returns a human-readable string representing the given context.
    fn to_human_with(&self, name: &str) -> String {
        if name == GLOBAL_CONTEXT {
            "the global scope".to_string()
        } else {
            format!("'{}'", name)
        }
    }
}
