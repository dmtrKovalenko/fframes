use std::{fmt, iter::FromIterator};
use usvgr::svgtree::{NestedNodeData, NestedSvgDocument};

#[derive(Default)]
pub struct Svgr {
    #[cfg(not(feature = "compile-time-svgtree"))]
    pub value: String,
    #[cfg(feature = "compile-time-svgtree")]
    pub svg_tree: NestedSvgDocument,
}

impl Svgr {
    // pub fn into_string(self) -> String {
    //     self.value
    // }

    #[cfg(feature = "compile-time-svgtree")]
    pub fn into_svg_tree(self, opt: &usvgr::Options) -> Result<usvgr::Tree, usvgr::Error> {
        usvgr::Tree::from_nested_svgtree(self.svg_tree, opt)
    }

    #[cfg(not(feature = "compile-time-svgtree"))]
    pub fn into_svg_tree(self, opt: &usvgr::Options) -> Result<usvgr::Tree, usvgr::Error> {
        usvgr::Tree::from_str(self.value.as_str(), opt)
    }
}

#[cfg(not(feature = "compile-time-svgtree"))]
impl From<String> for Svgr {
    fn from(value: String) -> Self {
        Svgr { value }
    }
}

#[cfg(feature = "compile-time-svgtree")]
impl From<String> for Svgr {
    fn from(val: String) -> Self {
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

#[cfg(not(feature = "compile-time-svgtree"))]
impl FromIterator<Svgr> for Svgr {
    fn from_iter<T: IntoIterator<Item = Svgr>>(iter: T) -> Self {
        Svgr {
            value: iter
                .into_iter()
                .fold(String::new(), |acc, s| acc + &s.value),
        }
    }
}

#[cfg(feature = "compile-time-svgtree")]
impl FromIterator<Svgr> for Svgr {
    fn from_iter<T: IntoIterator<Item = Svgr>>(iter: T) -> Self {
        let mut child_nodes = NestedSvgDocument { nodes: vec![] };

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

#[cfg(feature = "compile-time-svgtree")]
impl fmt::Display for Svgr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self.svg_tree)
    }
}

#[cfg(not(feature = "compile-time-svgtree"))]
impl fmt::Display for Svgr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.value)
    }
}
