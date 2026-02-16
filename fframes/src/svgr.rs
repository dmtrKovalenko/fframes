use crate::error::{FFramesError, Result};
use std::{fmt, iter::FromIterator};

#[derive(Default, Clone, Debug)]
pub struct Svgr<'a> {
    #[cfg(any(not(feature = "compile-time-svgtree"), target_arch = "wasm32"))]
    pub value: String,
    #[cfg(any(not(feature = "compile-time-svgtree"), target_arch = "wasm32"))]
    pub marker: std::marker::PhantomData<&'a str>,
    #[cfg(all(feature = "compile-time-svgtree", not(target_arch = "wasm32")))]
    pub svg_tree: usvgr::svgtree::NestedSvgDocument<'a, usvgr::svgtree::NestedNodeData<'a>>,
}

impl<'a> Svgr<'a> {
    #[cfg(all(feature = "compile-time-svgtree", not(target_arch = "wasm32")))]
    pub fn into_svg_tree(
        self,
        opt: &usvgr::Options,
        cache: &mut usvgr::Cache,
        fontdb: &usvgr::fontdb::Database,
    ) -> Result<usvgr::Tree> {
        usvgr::Tree::from_nested_svgtree_with_cache(&self.svg_tree, opt, cache, fontdb)
            .map_err(FFramesError::ParserError)
    }

    #[cfg(any(not(feature = "compile-time-svgtree"), target_arch = "wasm32"))]
    pub fn into_svg_tree(
        self,
        opt: &usvgr::Options,
        _cache: &mut usvgr::Cache,
        fontdb: &usvgr::fontdb::Database,
    ) -> Result<usvgr::Tree> {
        usvgr::Tree::from_str(&self.value, opt, fontdb).map_err(FFramesError::ParserError)
    }

    #[cfg(all(feature = "compile-time-svgtree", not(target_arch = "wasm32")))]
    pub fn as_subtree(self) -> Vec<Option<usvgr::svgtree::NestedNodeData<'a>>> {
        self.svg_tree.nodes
    }

    #[cfg(any(not(feature = "compile-time-svgtree"), target_arch = "wasm32"))]
    pub fn as_subtree(self) -> Vec<Option<usvgr::svgtree::NestedNodeData<'a>>> {
        unimplemented!(
            "Subtrees are not available when using runtime svg tree, if you see this message it means that feature flags are set incorrectly."
        )
    }

    pub fn empty() -> Self {
        Self::default()
    }
}

#[cfg(any(not(feature = "compile-time-svgtree"), target_arch = "wasm32"))]
impl From<String> for Svgr<'_> {
    fn from(value: String) -> Self {
        Svgr {
            value,
            marker: std::marker::PhantomData,
        }
    }
}

#[cfg(all(feature = "compile-time-svgtree", not(target_arch = "wasm32")))]
impl From<String> for Svgr<'_> {
    fn from(val: String) -> Self {
        use usvgr::svgtree::{NestedNodeData, NestedSvgDocument};

        Svgr {
            svg_tree: NestedSvgDocument::from_nodes(vec![
                Some(NestedNodeData {
                    kind: usvgr::svgtree::NestedNodeKind::Text(
                        usvgr::svgtree::roxmltree::StringStorage::new_owned(val)
                    ),
                    attrs: vec![],
                    children: vec![],
                    static_hash: None,
                });
                1
            ]),
        }
    }
}

#[cfg(any(not(feature = "compile-time-svgtree"), target_arch = "wasm32"))]
impl<'a> FromIterator<Svgr<'a>> for Svgr<'a> {
    fn from_iter<T: IntoIterator<Item = Svgr<'a>>>(iter: T) -> Self {
        Svgr {
            marker: std::marker::PhantomData,
            value: iter
                .into_iter()
                .fold(String::new(), |acc, s| acc + &s.value),
        }
    }
}

#[cfg(all(feature = "compile-time-svgtree", not(target_arch = "wasm32")))]
impl<'a> FromIterator<Svgr<'a>> for Svgr<'a> {
    fn from_iter<T: IntoIterator<Item = Svgr<'a>>>(iter: T) -> Self {
        let mut child_nodes = usvgr::svgtree::NestedSvgDocument::from_nodes(vec![]);

        for sub_tree in iter {
            let mut nested_tree = sub_tree.svg_tree;

            child_nodes.nodes.append(&mut nested_tree.nodes);
        }

        Svgr {
            svg_tree: child_nodes,
        }
    }
}

#[cfg(any(not(feature = "compile-time-svgtree"), target_arch = "wasm32"))]
impl<'a> From<&'a str> for Svgr<'a> {
    fn from(val: &'a str) -> Self {
        Svgr {
            value: val.to_string(),
            marker: std::marker::PhantomData,
        }
    }
}

#[cfg(all(feature = "compile-time-svgtree", not(target_arch = "wasm32")))]
impl<'a> From<&'a str> for Svgr<'a> {
    fn from(val: &'a str) -> Self {
        use usvgr::svgtree::{NestedNodeData, NestedSvgDocument};

        Svgr {
            svg_tree: NestedSvgDocument::from_nodes(vec![
                Some(NestedNodeData {
                    kind: usvgr::svgtree::NestedNodeKind::Text(
                        usvgr::svgtree::roxmltree::StringStorage::Borrowed(val)
                    ),
                    attrs: vec![],
                    children: vec![],
                    static_hash: None,
                });
                1
            ]),
        }
    }
}

impl<'a> From<Vec<Svgr<'a>>> for Svgr<'a> {
    fn from(val: Vec<Svgr<'a>>) -> Svgr<'a> {
        FromIterator::from_iter(val)
    }
}

#[cfg(all(feature = "compile-time-svgtree", not(target_arch = "wasm32")))]
impl fmt::Display for Svgr<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self.svg_tree)
    }
}

#[cfg(any(not(feature = "compile-time-svgtree"), target_arch = "wasm32"))]
impl fmt::Display for Svgr<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.value)
    }
}
