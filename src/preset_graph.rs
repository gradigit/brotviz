use std::collections::HashMap;
use std::fmt;
use std::path::Path;

#[derive(Debug, Clone, PartialEq)]
pub struct PresetGraph {
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphNode {
    pub id: String,
    pub preset_index: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GraphEdge {
    pub from: String,
    pub to: String,
    pub op: GraphOp,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GraphOp {
    Always,
    OnBeat,
    BeatStrengthGe(f32),
    RmsGe(f32),
    Chance(f32),
}

#[derive(Debug, Clone, PartialEq)]
pub struct CompiledPresetGraph {
    pub nodes: Vec<CompiledNode>,
    pub adjacency: Vec<Vec<CompiledEdge>>,
    pub entry: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompiledNode {
    pub id: String,
    pub preset_index: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CompiledEdge {
    pub to: usize,
    pub op: GraphOp,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PresetGraphError {
    Io(String),
    Parse { line: usize, message: String },
    EmptyGraph,
    DuplicateNodeId(String),
    UnknownNodeRef { edge: usize, node_id: String },
    CycleDetected { at: String },
}

impl fmt::Display for PresetGraphError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(msg) => write!(f, "I/O error: {msg}"),
            Self::Parse { line, message } => write!(f, "parse error at line {line}: {message}"),
            Self::EmptyGraph => write!(f, "graph must define at least one node"),
            Self::DuplicateNodeId(id) => write!(f, "duplicate node id: {id}"),
            Self::UnknownNodeRef { edge, node_id } => {
                write!(f, "edge #{edge} references unknown node '{node_id}'")
            }
            Self::CycleDetected { at } => write!(f, "cycle detected at node '{at}'"),
        }
    }
}

impl std::error::Error for PresetGraphError {}

impl PresetGraph {
    pub fn parse(text: &str) -> Result<Self, PresetGraphError> {
        let mut nodes = Vec::new();
        let mut edges = Vec::new();

        for (line_idx, raw) in text.lines().enumerate() {
            let line_no = line_idx + 1;
            let trimmed = raw.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }

            let tokens: Vec<&str> = trimmed.split_whitespace().collect();
            let head = tokens.first().copied().ok_or(PresetGraphError::Parse {
                line: line_no,
                message: "empty statement".to_string(),
            })?;

            match head {
                "node" => {
                    if tokens.len() != 3 {
                        return Err(PresetGraphError::Parse {
                            line: line_no,
                            message: "node expects: node <id> <preset_index>".to_string(),
                        });
                    }
                    let id = tokens[1].to_string();
                    if !is_ident(&id) {
                        return Err(PresetGraphError::Parse {
                            line: line_no,
                            message: format!("invalid node id: {id}"),
                        });
                    }
                    let preset_index =
                        parse_usize(tokens[2], line_no, "invalid preset index for node")?;
                    nodes.push(GraphNode { id, preset_index });
                }
                "edge" => {
                    if tokens.len() < 4 {
                        return Err(PresetGraphError::Parse {
                            line: line_no,
                            message: "edge expects: edge <from> <to> <op> [arg]".to_string(),
                        });
                    }
                    let from = tokens[1].to_string();
                    let to = tokens[2].to_string();
                    let op = parse_op(&tokens[3..], line_no)?;
                    edges.push(GraphEdge { from, to, op });
                }
                _ => {
                    return Err(PresetGraphError::Parse {
                        line: line_no,
                        message: format!("unknown statement '{head}'"),
                    });
                }
            }
        }

        Ok(Self { nodes, edges })
    }

    pub fn load(path: impl AsRef<Path>) -> Result<Self, PresetGraphError> {
        let text = std::fs::read_to_string(path.as_ref())
            .map_err(|e| PresetGraphError::Io(e.to_string()))?;
        Self::parse(&text)
    }

    pub fn compile(&self) -> Result<CompiledPresetGraph, PresetGraphError> {
        if self.nodes.is_empty() {
            return Err(PresetGraphError::EmptyGraph);
        }

        let mut id_to_idx = HashMap::new();
        let mut compiled_nodes = Vec::with_capacity(self.nodes.len());
        for (idx, node) in self.nodes.iter().enumerate() {
            if id_to_idx.insert(node.id.clone(), idx).is_some() {
                return Err(PresetGraphError::DuplicateNodeId(node.id.clone()));
            }
            compiled_nodes.push(CompiledNode {
                id: node.id.clone(),
                preset_index: node.preset_index,
            });
        }

        let mut adjacency: Vec<Vec<CompiledEdge>> = vec![Vec::new(); self.nodes.len()];
        for (edge_idx, edge) in self.edges.iter().enumerate() {
            let from_idx = id_to_idx
                .get(&edge.from)
                .copied()
                .ok_or_else(|| PresetGraphError::UnknownNodeRef {
                    edge: edge_idx,
                    node_id: edge.from.clone(),
                })?;
            let to_idx = id_to_idx
                .get(&edge.to)
                .copied()
                .ok_or_else(|| PresetGraphError::UnknownNodeRef {
                    edge: edge_idx,
                    node_id: edge.to.clone(),
                })?;
            adjacency[from_idx].push(CompiledEdge {
                to: to_idx,
                op: edge.op,
            });
        }

        if let Some(at) = detect_cycle(&compiled_nodes, &adjacency) {
            return Err(PresetGraphError::CycleDetected { at });
        }

        Ok(CompiledPresetGraph {
            nodes: compiled_nodes,
            adjacency,
            entry: 0,
        })
    }
}

fn parse_op(tokens: &[&str], line: usize) -> Result<GraphOp, PresetGraphError> {
    let op_name = tokens[0];
    match op_name {
        "always" => expect_no_extra(tokens, line, GraphOp::Always),
        "on_beat" => expect_no_extra(tokens, line, GraphOp::OnBeat),
        "beat_ge" => {
            let v = parse_unit_interval(tokens, line, "beat_ge expects a value in [0,1]")?;
            Ok(GraphOp::BeatStrengthGe(v))
        }
        "rms_ge" => {
            let v = parse_unit_interval(tokens, line, "rms_ge expects a value in [0,1]")?;
            Ok(GraphOp::RmsGe(v))
        }
        "chance" => {
            let v = parse_unit_interval(tokens, line, "chance expects a value in [0,1]")?;
            Ok(GraphOp::Chance(v))
        }
        _ => Err(PresetGraphError::Parse {
            line,
            message: format!("unknown edge op '{op_name}'"),
        }),
    }
}

fn expect_no_extra(tokens: &[&str], line: usize, op: GraphOp) -> Result<GraphOp, PresetGraphError> {
    if tokens.len() != 1 {
        return Err(PresetGraphError::Parse {
            line,
            message: format!("op '{}' does not accept an argument", tokens[0]),
        });
    }
    Ok(op)
}

fn parse_unit_interval(tokens: &[&str], line: usize, msg: &str) -> Result<f32, PresetGraphError> {
    if tokens.len() != 2 {
        return Err(PresetGraphError::Parse {
            line,
            message: msg.to_string(),
        });
    }
    let v = tokens[1].parse::<f32>().map_err(|_| PresetGraphError::Parse {
        line,
        message: msg.to_string(),
    })?;
    if !v.is_finite() || !(0.0..=1.0).contains(&v) {
        return Err(PresetGraphError::Parse {
            line,
            message: msg.to_string(),
        });
    }
    Ok(v)
}

fn parse_usize(s: &str, line: usize, msg: &str) -> Result<usize, PresetGraphError> {
    s.parse::<usize>().map_err(|_| PresetGraphError::Parse {
        line,
        message: msg.to_string(),
    })
}

fn is_ident(s: &str) -> bool {
    let mut chars = s.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !(first.is_ascii_alphabetic() || first == '_') {
        return false;
    }
    chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '-')
}

fn detect_cycle(nodes: &[CompiledNode], adjacency: &[Vec<CompiledEdge>]) -> Option<String> {
    let mut color = vec![0u8; nodes.len()];
    for idx in 0..nodes.len() {
        if color[idx] == 0 && dfs_cycle(idx, nodes, adjacency, &mut color) {
            return Some(nodes[idx].id.clone());
        }
    }
    None
}

fn dfs_cycle(
    idx: usize,
    nodes: &[CompiledNode],
    adjacency: &[Vec<CompiledEdge>],
    color: &mut [u8],
) -> bool {
    color[idx] = 1;
    for edge in &adjacency[idx] {
        if color[edge.to] == 1 {
            return true;
        }
        if color[edge.to] == 0 && dfs_cycle(edge.to, nodes, adjacency, color) {
            return true;
        }
    }
    color[idx] = 2;
    let _ = nodes;
    false
}
