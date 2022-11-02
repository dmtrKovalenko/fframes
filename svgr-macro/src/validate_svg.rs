use crate::node::NodeName;
use syn::{parse::ParseBuffer, Result};

const UNSUPPORTED_NODES: [&str; 9] = [
    "altGlyph",
    "altGlyphDef",
    "animate",
    "animateColor",
    "animateMotion",
    "animateTransform",
    "hkern",
    "vkern",
    "foreignObject",
];

pub(crate) fn validate_node(input: &ParseBuffer, node: &NodeName) -> Result<()> {
    match node.to_string() {
        name if UNSUPPORTED_NODES.contains(&name.as_str()) => {
            Err(input.error(format!("Element <{name} /> is not supported")))
        }
        _ => Ok(()),
    }
}

const UNSUPPORTED_ATTRS: [&str; 1] = ["dominant-baseline"];

pub(crate) fn validate_attribute(
    input: &ParseBuffer,
    node: &NodeName,
    tag_name: &NodeName,
    is_block: bool,
) -> Result<()> {
    match (node.to_string(), tag_name.to_string()) {
        (attr, tag) if attr == "xlink:href" && tag == "image" && !is_block => Err(input.error(
            "Instead of hardcoding images please use xlink:href={ctx.get_image_link(\"image.png\"}",
        )),
        (attr, _) if attr == "href" => {
            Err(input.error("href attribute is not supported, use xlink:href"))
        }
        (attr, _) if UNSUPPORTED_ATTRS.contains(&attr.as_str()) => {
            Err(input.error(format!("attribute {attr} is not supported")))
        }
        _ => Ok(()),
    }
}
