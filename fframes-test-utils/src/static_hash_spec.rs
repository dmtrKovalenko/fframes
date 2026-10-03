//! The `svgr!` macro assigns a `static_hash` to nodes whose rendering is fully
//! known at compile time.  Renderers cache by that hash, so a node must only
//! get one when nothing that influences its output can change between frames.

use fframes::{Svgr, usvgr};
use std::collections::HashMap;

fn hashes_by_id(svgr: Svgr) -> HashMap<String, Option<u64>> {
    let tree = svgr
        .into_svg_tree(
            &usvgr::Options::default(),
            &mut usvgr::Cache::default(),
            &usvgr::fontdb::Database::default(),
        )
        .unwrap();

    let mut out = HashMap::new();
    collect(tree.root(), &mut out);
    out
}

#[test]
fn static_nodes_get_a_hash_and_dynamic_nodes_do_not() {
    let width = 120;
    let hashes = hashes_by_id(fframes::svgr!(
        <svg xmlns="http://www.w3.org/2000/svg" width="200" height="200">
            <rect id="static" x="10" y="10" width="50" height="50" fill="red" />
            <rect id="dynamic" x="10" y="10" width={width} height="50" fill="red" />
            <g id="static_group">
                <rect x="10" y="10" width="50" height="50" fill="red" />
                <circle cx="20" cy="20" r="5" fill="blue" />
            </g>
            <g id="group_with_dynamic_child">
                <rect x="10" y="10" width={width} height="50" fill="red" />
            </g>
        </svg>
    ));

    assert!(hashes["static"].is_some());
    assert!(hashes["dynamic"].is_none());
    assert!(hashes["static_group"].is_some());
    assert!(hashes["group_with_dynamic_child"].is_none());
}

#[test]
fn dynamic_inherited_attributes_poison_descendants() {
    let color = "#ff0000";
    let transform = fframes::Transform::translate(10, 10);
    let hashes = hashes_by_id(fframes::svgr!(
        <svg xmlns="http://www.w3.org/2000/svg" width="200" height="200">
            <g fill={color}>
                <rect id="inherits_fill" x="10" y="10" width="50" height="50" />
                <g id="nested">
                    <rect x="10" y="10" width="50" height="50" />
                </g>
            </g>
            // transform and opacity are not inherited: children stay static
            <g transform={transform}>
                <rect id="under_transform" x="10" y="10" width="50" height="50" fill="red" />
            </g>
            <g opacity={0.5}>
                <rect id="under_opacity" x="10" y="10" width="50" height="50" fill="red" />
            </g>
        </svg>
    ));

    assert!(hashes["inherits_fill"].is_none());
    assert!(hashes["nested"].is_none());
    assert!(hashes["under_transform"].is_some());
    assert!(hashes["under_opacity"].is_some());
}

#[test]
fn percentage_lengths_under_a_dynamic_viewport_are_not_static() {
    let width = 400;
    let hashes = hashes_by_id(fframes::svgr!(
        <svg xmlns="http://www.w3.org/2000/svg" width={width} height="200">
            <rect id="percent" x="0" y="0" width="50%" height="10" fill="red" />
            <rect id="absolute" x="0" y="0" width="50" height="10" fill="red" />
        </svg>
    ));

    assert!(hashes["percent"].is_none());
    assert!(hashes["absolute"].is_some());
}

#[test]
fn references_are_resolved_within_the_invocation() {
    let color = "#ff0000";
    let hashes = hashes_by_id(fframes::svgr!(
        <svg xmlns="http://www.w3.org/2000/svg" width="200" height="200">
            <defs>
                <linearGradient id="static_gradient">
                    <stop offset="0" stop-color="red" />
                    <stop offset="1" stop-color="blue" />
                </linearGradient>
                <linearGradient id="dynamic_gradient">
                    <stop offset="0" stop-color={color} />
                    <stop offset="1" stop-color="blue" />
                </linearGradient>
                <rect id="symbol" x="0" y="0" width="20" height="20" fill="green" />
                <clipPath id="clip">
                    <rect x="0" y="0" width="20" height="20" />
                </clipPath>
            </defs>
            <rect id="static_ref" x="0" y="0" width="50" height="50" fill="url(#static_gradient)" />
            <rect id="dynamic_ref" x="0" y="0" width="50" height="50" fill="url(#dynamic_gradient)" />
            <rect id="unknown_ref" x="0" y="0" width="50" height="50" fill="url(#defined_elsewhere)" stroke="black" />
            <g id="clipped" clip-path="url(#clip)">
                <rect x="0" y="0" width="50" height="50" fill="red" />
            </g>
            <use id="static_use" href="#symbol" x="10" y="10" />
        </svg>
    ));

    assert!(hashes["static_ref"].is_some());
    assert!(hashes["dynamic_ref"].is_none());
    assert!(hashes["unknown_ref"].is_none());
    assert!(hashes["clipped"].is_some());
    assert!(hashes["static_use"].is_some());
}

#[test]
fn identical_nodes_referencing_different_definitions_hash_differently() {
    let first = hashes_by_id(fframes::svgr!(
        <svg xmlns="http://www.w3.org/2000/svg" width="200" height="200">
            <defs>
                <linearGradient id="g">
                    <stop offset="0" stop-color="red" />
                </linearGradient>
            </defs>
            <rect id="r" x="0" y="0" width="50" height="50" fill="url(#g)" />
        </svg>
    ));
    let second = hashes_by_id(fframes::svgr!(
        <svg xmlns="http://www.w3.org/2000/svg" width="200" height="200">
            <defs>
                <linearGradient id="g">
                    <stop offset="0" stop-color="blue" />
                </linearGradient>
            </defs>
            <rect id="r" x="0" y="0" width="50" height="50" fill="url(#g)" />
        </svg>
    ));

    assert!(first["r"].is_some());
    assert!(second["r"].is_some());
    assert_ne!(first["r"], second["r"]);
}

fn collect(group: &usvgr::Group, out: &mut HashMap<String, Option<u64>>) {
    for node in group.children() {
        match node {
            usvgr::Node::Group(g) => {
                if !g.id().is_empty() {
                    out.insert(g.id().to_owned(), g.static_hash());
                }
                collect(g, out);
            }
            usvgr::Node::Path(p) if !p.id().is_empty() => {
                out.insert(p.id().to_owned(), p.static_hash());
            }
            usvgr::Node::FastShape(e) if !e.path().id().is_empty() => {
                out.insert(e.path().id().to_owned(), e.path().static_hash());
            }
            _ => {}
        }
    }
}
