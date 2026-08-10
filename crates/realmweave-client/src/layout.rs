//! Node layout for the two views. Pure math, no Bevy dependency beyond Vec3
//! being trivially constructible by the caller (we return [f32; 3]).

use realmweave_core::{BoardGraph, NodeId, Realm};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ViewMode {
    /// Three stacked realm layers (the product view).
    #[default]
    Stacked3D,
    /// Three realms side by side (mandatory analysis view).
    Analysis2D,
}

impl ViewMode {
    pub fn toggle(self) -> Self {
        match self {
            ViewMode::Stacked3D => ViewMode::Analysis2D,
            ViewMode::Analysis2D => ViewMode::Stacked3D,
        }
    }
}

/// Horizontal spread between realm centers in the 2D analysis view.
fn analysis_offset(board: &BoardGraph) -> f32 {
    // Wide enough for the largest board: max |x| + margin.
    let max_x = board
        .definition()
        .nodes
        .iter()
        .map(|n| n.position[0].abs())
        .fold(0.0f32, f32::max);
    max_x * 2.0 + 3.0
}

/// World position of a node in the given view mode.
pub fn node_position(board: &BoardGraph, node: NodeId, mode: ViewMode) -> [f32; 3] {
    let def = &board.definition().nodes[node as usize];
    let [x, y, z] = def.position;
    match mode {
        ViewMode::Stacked3D => [x, y, z],
        ViewMode::Analysis2D => {
            // Merged-field boards (triforce) are ONE flat triangle whose
            // realm tags are interior regions — shifting per tag would tear
            // the board into three pieces. They render as-is.
            if board.definition().id.starts_with("tf") {
                return [x, 0.0, z];
            }
            let dx = analysis_offset(board);
            let shift = match def.realm {
                Realm::Heaven => -dx,
                Realm::Mortal => 0.0,
                Realm::Underworld => dx,
            };
            // Flatten to one plane, spread horizontally.
            [x + shift, 0.0, z]
        }
    }
}
