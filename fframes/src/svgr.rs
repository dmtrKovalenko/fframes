use crate::error::{FFramesError, Result};
use std::{fmt, iter::FromIterator};

#[derive(Default, Clone)]
pub struct Svgr {
    #[cfg(any(not(feature = "compile-time-svgtree"), target_arch = "wasm32"))]
    pub value: String,
    #[cfg(all(feature = "compile-time-svgtree", not(target_arch = "wasm32")))]
    pub svg_tree: usvgr::svgtree::NestedSvgDocument<usvgr::svgtree::NestedNodeData>,
}

impl Svgr {
    #[cfg(all(feature = "compile-time-svgtree", not(target_arch = "wasm32")))]
    pub fn into_svg_tree(self, opt: &usvgr::Options) -> Result<usvgr::Tree> {
        usvgr::Tree::from_nested_svgtree(self.svg_tree, opt).map_err(FFramesError::ParserError)
    }

    #[cfg(any(not(feature = "compile-time-svgtree"), target_arch = "wasm32"))]
    pub fn into_svg_tree(self, opt: &usvgr::Options) -> Result<usvgr::Tree> {
        usvgr::Tree::from_str(self.value.as_str(), opt).map_err(FFramesError::ParserError)
    }

    #[cfg(all(feature = "compile-time-svgtree", not(target_arch = "wasm32")))]
    pub fn as_subtree(self) -> Vec<Option<usvgr::svgtree::NestedNodeData>> {
        self.svg_tree.nodes
    }

    #[cfg(any(not(feature = "compile-time-svgtree"), target_arch = "wasm32"))]
    pub fn as_subtree(self) -> Vec<Option<usvgr::svgtree::NestedNodeData>> {
        unimplemented!("Subtrees are not available when using runtime svg tree, if you see this message it means that feature flags are set incorrectly.")
    }
}

#[cfg(any(not(feature = "compile-time-svgtree"), target_arch = "wasm32"))]
impl From<String> for Svgr {
    fn from(value: String) -> Self {
        Svgr { value }
    }
}

#[cfg(all(feature = "compile-time-svgtree", not(target_arch = "wasm32")))]
impl From<String> for Svgr {
    fn from(val: String) -> Self {
        use usvgr::svgtree::{NestedNodeData, NestedSvgDocument};

        Svgr {
            svg_tree: NestedSvgDocument {
                nodes: vec![Some(NestedNodeData {
                    kind: usvgr::svgtree::NestedNodeKind::Text(val),
                    attrs: vec![],
                    children: vec![],
                })],
            },
        }
    }
}

#[cfg(any(not(feature = "compile-time-svgtree"), target_arch = "wasm32"))]
impl FromIterator<Svgr> for Svgr {
    fn from_iter<T: IntoIterator<Item = Svgr>>(iter: T) -> Self {
        Svgr {
            value: iter
                .into_iter()
                .fold(String::new(), |acc, s| acc + &s.value),
        }
    }
}

#[cfg(all(feature = "compile-time-svgtree", not(target_arch = "wasm32")))]
impl FromIterator<Svgr> for Svgr {
    fn from_iter<T: IntoIterator<Item = Svgr>>(iter: T) -> Self {
        let mut child_nodes = usvgr::svgtree::NestedSvgDocument { nodes: vec![] };

        for sub_tree in iter {
            let mut nested_tree = sub_tree.svg_tree;

            child_nodes.nodes.append(&mut nested_tree.nodes);
        }

        Svgr {
            svg_tree: child_nodes,
        }
    }
}

impl<'a> From<&'a str> for Svgr {
    fn from(val: &'a str) -> Self {
        Svgr::from(String::from(val))
    }
}

impl From<Vec<Svgr>> for Svgr {
    fn from(val: Vec<Svgr>) -> Svgr {
        FromIterator::from_iter(val)
    }
}

#[cfg(all(feature = "compile-time-svgtree", not(target_arch = "wasm32")))]
impl fmt::Display for Svgr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self.svg_tree)
    }
}

#[cfg(any(not(feature = "compile-time-svgtree"), target_arch = "wasm32"))]
impl fmt::Display for Svgr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.value)
    }
}
