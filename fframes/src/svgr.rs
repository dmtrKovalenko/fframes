use std::{fmt, iter::FromIterator};
use usvgr::svgtree::{Document, NestedSvgDocument};

/// This struct should represent the svg AST. WIP.
#[derive(Default)]
pub struct Svgr {
    pub value: String,
    pub svg_tree: Option<NestedSvgDocument>,
}

impl Svgr {
    pub fn into_string(self) -> String {
        self.value
    }
}

pub trait IntoSvgr {
    fn into_svgr(self) -> Svgr;
}

impl From<String> for Svgr {
    fn from(val: String) -> Self {
        Svgr { value: val, svg_tree: None }
    }
}

impl<'a> From<&'a str> for Svgr {
    fn from(val: &'a str) -> Self {
        Svgr {
            value: val.to_owned(),
            svg_tree: None
        }
    }
}

impl From<Vec<Svgr>> for Svgr {
    fn from(val: Vec<Svgr>) -> Svgr {
        FromIterator::from_iter(val)
    }
}

impl FromIterator<Svgr> for Svgr {
    fn from_iter<T: IntoIterator<Item = Svgr>>(iter: T) -> Self {
        Svgr {
            svg_tree: None,
            value: iter
                .into_iter()
                .fold(String::new(), |acc, s| acc + &s.value),
        }
    }
}

impl fmt::Display for Svgr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.value)
    }
}
