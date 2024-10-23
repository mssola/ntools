use crate::assembler::Bundle;

lazy_static! {
    pub static ref EMPTY: Vec<Segment> = vec![Segment {
        name: String::from("CODE"),
        start: 0x0000,
        size: 0xFFFF,
        offset: 0,
        fill: None,
        bundles: vec![],
    }];
    pub static ref NROM: Vec<Segment> = vec![
        Segment {
            name: String::from("HEADER"),
            start: 0x0000,
            size: 0x0010,
            offset: 0,
            fill: Some(0x00),
            bundles: vec![],
        },
        Segment {
            name: String::from("VECTORS"),
            start: 0xFFFA,
            size: 0x0006,
            offset: 0,
            fill: Some(0x00),
            bundles: vec![],
        },
        Segment {
            name: String::from("CODE"),
            start: 0x8000,
            size: 0x7FFA,
            offset: 0,
            fill: Some(0x00),
            bundles: vec![],
        },
        Segment {
            name: String::from("CHARS"),
            start: 0x0000,
            size: 0x2000,
            offset: 0,
            fill: Some(0x00),
            bundles: vec![],
        }
    ];
}

#[derive(Debug, Clone, Eq, Ord, PartialEq, PartialOrd)]
pub struct Segment {
    pub name: String,
    pub start: u16,
    pub size: usize,
    pub offset: usize,
    pub fill: Option<usize>,
    pub bundles: Vec<Bundle>,
}

/* TODO
#[derive(Debug)]
pub struct Mapping {
    pub segments: Vec<Segment>,
    pub nodes: HashMap<String, Vec<Node>>,
    pub current: String,
    pub macros: HashMap<String, Vec<Node>>,
    pub current_macro: Option<String>,
}

impl Mapping {
    pub fn new(mut segments: Vec<Segment>) -> Self {
        segments.sort_by(|a, b| a.start.cmp(&b.start));

        let mut nodes = HashMap::new();
        for segment in segments.iter() {
            nodes.insert(segment.name.clone(), vec![]);
        }

        let current_segment = &segments.first().unwrap().name.clone();

        Mapping {
            segments,
            nodes,
            current: current_segment.to_string(),
            macros: HashMap::new(),
            current_macro: None,
        }
    }

    pub fn reset(&mut self) {
        self.nodes = HashMap::new();
        for segment in self.segments.iter() {
            self.nodes.insert(segment.name.clone(), vec![]);
        }

        self.current = self.segments.first().unwrap().name.clone();

        self.macros = HashMap::new();
        self.current_macro = None;
    }

    pub fn switch(&mut self, id: &PString) -> Result<()> {
        if !self.nodes.contains_key(&id.value) {
            // TODO
            // return Err(
            //     id.parser_error(format!("segment '{}' has not been defined", id.value).as_str())
            // );
        }

        id.value.clone_into(&mut self.current);
        Ok(())
    }

    pub fn current(&self) -> &Vec<Node> {
        self.nodes.get(&self.current).unwrap()
    }

    pub fn current_mut(&mut self) -> &mut Vec<Node> {
        self.nodes.get_mut(&self.current).unwrap()
    }

    pub fn push(&mut self, node: Node) {
        match &self.current_macro {
            Some(m) => self.macros.get_mut(m).unwrap().push(node),
            None => self.nodes.get_mut(&self.current).unwrap().push(node),
        }
    }
}
*/
