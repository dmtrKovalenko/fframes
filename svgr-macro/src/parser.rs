use std::{cell::RefCell, rc::Rc};

use proc_macro2::{Span, TokenStream, TokenTree};
use syn::{
    braced,
    ext::IdentExt,
    parse::{discouraged::Speculative, Parse, ParseStream, Parser as _, Peek},
    punctuated::Punctuated,
    token::{Brace, Colon, Colon2},
    Block, Error, Expr, ExprBlock, ExprLit, ExprPath, Ident, Path, PathArguments, PathSegment,
    Result, Stmt, Token,
};

use crate::{node::*, punctuation::*};

type TransformBlockFn = dyn Fn(ParseStream) -> Result<Option<TokenStream>>;

/// Configures the `Parser` behavior
#[derive(Default)]
pub struct ParserOptions {
    number_of_top_level_nodes: Option<usize>,
    transform_block: Option<Box<TransformBlockFn>>,
}

pub struct Parser<'a> {
    config: ParserOptions,
    fframes_crate_ident: &'a Ident,
    animation_attributes: Rc<RefCell<Vec<TokenStream>>>,
}

impl<'a> Parser<'a> {
    /// Create a new parser with the given config
    pub fn new(config: ParserOptions, fframes_crate_ident: &Ident) -> Parser {
        Parser {
            config,
            fframes_crate_ident,
            animation_attributes: Rc::new(RefCell::new(vec![])),
        }
    }

    /// Parse a given `syn::ParseStream`
    pub fn parse(&self, input: ParseStream) -> Result<(Vec<Node>, Vec<TokenStream>)> {
        let mut nodes = vec![];
        let mut top_level_nodes = 0;
        while !input.cursor().eof() {
            let parsed_nodes = &mut self.node(input)?;

            nodes.append(parsed_nodes);
            top_level_nodes += 1;
        }

        if let Some(number_of_top_level_nodes) = &self.config.number_of_top_level_nodes {
            if &top_level_nodes != number_of_top_level_nodes {
                return Err(input.error(format!(
                    "saw {top_level_nodes} top level nodes but exactly {number_of_top_level_nodes} are required",
                )));
            }
        }

        Ok((nodes, self.animation_attributes.borrow().clone()))
    }

    fn node(&self, input: ParseStream) -> Result<Vec<Node>> {
        let node = if input.peek(Token![<]) {
            self.element(input)
        } else if input.peek(Brace) {
            self.block(input)
        } else {
            self.text(input)
        }?;

        Ok(vec![node])
    }

    fn text(&self, input: ParseStream) -> Result<Node> {
        let text = input.parse::<ExprLit>()?.into();

        Ok(Node {
            name: None,
            value: Some(text),
            node_type: NodeType::Text,
            attributes: vec![],
            children: vec![],
        })
    }

    fn block(&self, input: ParseStream) -> Result<Node> {
        let (block, node_type) = if self.config.transform_block.is_some() {
            (self.block_transform(input)?, NodeType::Block)
        } else {
            self.block_expr(input)?
        };

        Ok(Node {
            name: None,
            value: Some(block),
            node_type,
            attributes: vec![],
            children: vec![],
        })
    }

    fn block_transform(&self, input: ParseStream) -> Result<Expr> {
        let transform_block = self.config.transform_block.as_ref().unwrap();

        input.step(|cursor| {
            if let Some((tree, next)) = cursor.token_tree() {
                match tree {
                    TokenTree::Group(block_group) => {
                        let block_span = block_group.span();
                        let parser = move |block_content: ParseStream| match transform_block(
                            block_content,
                        ) {
                            Ok(transformed_tokens) => match transformed_tokens {
                                Some(tokens) => {
                                    let parser = move |input: ParseStream| {
                                        Ok(self.block_content_to_block(input, block_span))
                                    };
                                    parser.parse2(tokens)?
                                }
                                None => self.block_content_to_block(block_content, block_span),
                            },
                            Err(error) => Err(error),
                        };
                        Ok((parser.parse2(block_group.stream())?, next))
                    }
                    _ => Err(cursor.error("unexpected: no Group in TokenTree found")),
                }
            } else {
                Err(cursor.error("unexpected: no TokenTree found"))
            }
        })
    }

    fn block_content_to_block(&self, input: ParseStream, span: Span) -> Result<Expr> {
        Ok(ExprBlock {
            attrs: vec![],
            label: None,
            block: Block {
                brace_token: Brace { span },
                stmts: Block::parse_within(input)?,
            },
        }
        .into())
    }

    fn process_block(&self, statements: &[Stmt]) -> Option<Vec<Stmt>> {
        use quote::quote;
        use Stmt::*;

        let first_statement = (statements.len() == 1).then(|| &statements[0]);
        if let Some(Expr(syn::Expr::MethodCall(method_call))) = first_statement {
            if method_call.method == "animate" && method_call.args.len() == 1 {
                match &method_call.args[0] {
                    syn::Expr::Macro(macro_expr)
                        if macro_expr
                            .mac
                            .path
                            .segments
                            .iter()
                            .any(|segment| segment.ident == "timeline") =>
                    {
                        let mut punctuated = Punctuated::new();

                        punctuated.push(PathSegment {
                            ident: Ident::new(
                                &format!("ANIMATION_{}", uuid::Uuid::new_v4().to_simple()),
                                Span::call_site(),
                            ),
                            arguments: PathArguments::None,
                        });

                        let identifier = syn::Expr::Path(ExprPath {
                            path: Path {
                                leading_colon: None,
                                segments: punctuated,
                            },
                            attrs: vec![],
                            qself: None,
                        });

                        let fframes_crate_ident = self.fframes_crate_ident;
                        let first_animation_value = macro_expr
                            .mac
                            .tokens
                            .clone()
                            .into_iter()
                            .skip_while(|el| match el {
                                proc_macro2::TokenTree::Ident(ident) => *ident != "val",
                                _ => true,
                            })
                            .nth(1);

                        // We only support the color and f32 as animation params so here we are doing a very unsafe assumption that
                        // any literal is an f32 and everything else is a color. I do not want to pass additional types at to the macro
                        // so let's check how it will work for now and would real users have any problems with this.
                        let animation_type =  match first_animation_value {
                            Some(proc_macro2::TokenTree::Punct(val)) if val.as_char() == '-' => quote! { f32 },
                            Some(proc_macro2::TokenTree::Literal(_)) => quote! { f32 },
                            Some(proc_macro2::TokenTree::Ident(_)) => quote! { #fframes_crate_ident::Color },
                            _ => panic!("Can not infer the type of animation value. Did you set something else than a f32 or fframes::Color as the animation value? {:?}", first_animation_value),
                        };

                        self.animation_attributes.borrow_mut().push(quote::quote! {
                            static ref #identifier: #fframes_crate_ident::animation::SteppedAnimation<#animation_type> = #macro_expr;
                        });

                        let mut processed_method_call = method_call.clone();
                        processed_method_call.args = method_call
                            .args
                            .iter()
                            .map(|_| {
                                syn::Expr::Reference(syn::ExprReference {
                                    and_token: Default::default(),
                                    raw: Default::default(),
                                    attrs: Default::default(),
                                    mutability: None,
                                    expr: Box::new(identifier.clone()),
                                })
                            })
                            .collect();

                        return Some(vec![syn::Stmt::Expr(syn::Expr::MethodCall(
                            processed_method_call,
                        ))]);
                    }
                    _ => (),
                }
            }
        }

        None
    }

    fn block_expr(&self, input: ParseStream) -> Result<(Expr, NodeType)> {
        let fork = input.fork();

        let content;
        let brace_token = braced!(content in fork);

        let block = ExprBlock {
            attrs: vec![],
            label: None,
            block: Block {
                brace_token,
                stmts: Block::parse_within(&content)?,
            },
        };

        input.advance_to(&fork);

        Ok((block.into(), NodeType::Block))
    }

    fn block_attribute_expr(&self, input: ParseStream) -> Result<(Expr, NodeType)> {
        let fork = input.fork();

        let content;
        let brace_token = braced!(content in fork);
        let statements = Block::parse_within(&content)?;
        let statements = self
            .process_block(statements.as_slice())
            .unwrap_or(statements);

        let block = ExprBlock {
            attrs: vec![],
            label: None,
            block: Block {
                brace_token,
                stmts: statements,
            },
        };

        input.advance_to(&fork);

        Ok((block.into(), NodeType::Attribute))
    }

    fn element(&self, input: ParseStream) -> Result<Node> {
        let fork = &input.fork();

        if self.tag_close(&input.fork()).is_ok() {
            return Err(fork.error("close tag has no corresponding open tag"));
        }

        let (name, attributes, self_closing) = self.tag_open(fork)?;
        let mut children = vec![];
        if !self_closing {
            loop {
                if !self.element_has_children(&name, fork)? {
                    break;
                }

                children.append(&mut self.node(fork)?);
            }

            self.tag_close(fork)?;
        }
        input.advance_to(fork);

        Ok(Node {
            name: Some(name),
            value: None,
            node_type: NodeType::Element,
            attributes,
            children,
        })
    }

    fn element_has_children(&self, tag_open_name: &NodeName, input: ParseStream) -> Result<bool> {
        // an empty input at this point means the tag wasn't closed
        if input.is_empty() {
            return Err(Error::new(
                tag_open_name.span(),
                "open tag has no corresponding close tag and is not self-closing",
            ));
        }

        if let Ok(tag_close_name) = self.tag_close(&input.fork()) {
            if tag_open_name == &tag_close_name {
                // if the next token is a matching close tag then there are no child nodes
                return Ok(false);
            } else {
                // if the next token is a closing tag with a different name it's an invalid tree
                return Err(input.error("close tag has no corresponding open tag"));
            }
        }

        Ok(true)
    }

    fn tag_open(&self, input: ParseStream) -> Result<(NodeName, Vec<Node>, bool)> {
        input.parse::<Token![<]>()?;
        let tag_name = self.node_name(input)?;

        let mut attributes = TokenStream::new();
        let self_closing = loop {
            if let Ok(self_closing) = self.tag_open_end(input) {
                break self_closing;
            }

            if input.is_empty() {
                return Err(input.error("expected closing caret >"));
            }

            let next: TokenTree = input.parse()?;
            attributes.extend(Some(next));
        };

        let attributes = if !attributes.is_empty() {
            let tag_name = &tag_name;
            let parser = move |input: ParseStream| self.attributes(input, tag_name);
            parser.parse2(attributes)?
        } else {
            vec![]
        };

        Ok((tag_name, attributes, self_closing))
    }

    fn tag_open_end(&self, input: ParseStream) -> Result<bool> {
        let self_closing = input.parse::<Option<Token![/]>>()?.is_some();
        input.parse::<Token![>]>()?;

        Ok(self_closing)
    }

    fn tag_close(&self, input: ParseStream) -> Result<NodeName> {
        input.parse::<Token![<]>()?;
        input.parse::<Token![/]>()?;
        let name = self.node_name(input)?;
        input.parse::<Token![>]>()?;

        Ok(name)
    }

    fn attributes(&self, input: ParseStream, tag_name: &NodeName) -> Result<Vec<Node>> {
        let mut nodes = vec![];

        loop {
            if input.is_empty() {
                break;
            }

            nodes.push(self.attribute(input, tag_name)?);
        }

        Ok(nodes)
    }

    fn attribute(&self, input: ParseStream, _tag_name: &NodeName) -> Result<Node> {
        let fork = &input.fork();

        if fork.peek(Brace) {
            let (value, node_type) = self.block_expr(fork)?;
            input.advance_to(fork);

            Ok(Node {
                name: None,
                node_type,
                value: Some(value),
                attributes: vec![],
                children: vec![],
            })
        } else {
            let name = self.node_name(fork)?;

            let res = fork
                .parse::<Option<Token![=]>>()?
                .map(|_eq| {
                    if fork.is_empty() {
                        return Err(Error::new(name.span(), "missing attribute value"));
                    }

                    if fork.peek(Brace) {
                        Ok(self.block_attribute_expr(fork)?)
                    } else {
                        Ok((fork.parse()?, NodeType::Attribute))
                    }
                })
                .transpose()?;

            let (value, node_type) = if let Some((expr, node_type)) = res {
                (Some(expr), node_type)
            } else {
                (None, NodeType::Attribute)
            };

            input.advance_to(fork);
            Ok(Node {
                name: Some(name),
                value,
                node_type,
                attributes: vec![],
                children: vec![],
            })
        }
    }

    fn node_name(&self, input: ParseStream) -> Result<NodeName> {
        if input.peek2(Colon2) {
            self.node_name_punctuated_ident::<Colon2, fn(_) -> Colon2, PathSegment>(input, Colon2)
                .map(|segments| {
                    NodeName::Path(ExprPath {
                        attrs: vec![],
                        qself: None,
                        path: Path {
                            leading_colon: None,
                            segments,
                        },
                    })
                })
        } else if input.peek2(Colon) {
            self.node_name_punctuated_ident::<Colon, fn(_) -> Colon, Ident>(input, Colon)
                .map(NodeName::Colon)
        } else if input.peek2(Dash) {
            self.node_name_punctuated_ident::<Dash, fn(_) -> Dash, Ident>(input, Dash)
                .map(NodeName::Dash)
        } else if input.peek(Brace) {
            let fork = &input.fork();
            let (value, _) = self.block_expr(fork)?;
            input.advance_to(fork);

            Ok(NodeName::Block(value))
        } else if input.peek(Ident::peek_any) {
            let mut segments = Punctuated::new();
            let ident = Ident::parse_any(input)?;

            segments.push_value(PathSegment::from(ident));
            Ok(NodeName::Path(ExprPath {
                attrs: vec![],
                qself: None,
                path: Path {
                    leading_colon: None,
                    segments,
                },
            }))
        } else {
            Err(input.error("invalid tag name or attribute key"))
        }
    }

    // we can't replace this with [`Punctuated::parse_separated_nonempty`] since
    // that doesn't support reserved keywords. might be worth to consider a PR
    // upstream
    //
    // [`Punctuated::parse_separated_nonempty`]: https://docs.rs/syn/1.0.58/syn/punctuated/struct.Punctuated.html#method.parse_separated_nonempty
    fn node_name_punctuated_ident<T: Parse, F: Peek, X: From<Ident>>(
        &self,
        input: ParseStream,
        punct: F,
    ) -> Result<Punctuated<X, T>> {
        let fork = &input.fork();
        let mut segments = Punctuated::<X, T>::new();

        while !fork.is_empty() && fork.peek(Ident::peek_any) {
            let ident = Ident::parse_any(fork)?;
            segments.push_value(ident.clone().into());

            if fork.peek(punct) {
                segments.push_punct(fork.parse()?);
            } else {
                break;
            }
        }

        if segments.len() > 1 {
            input.advance_to(fork);
            Ok(segments)
        } else {
            Err(fork.error("expected punctuated node name"))
        }
    }
}
