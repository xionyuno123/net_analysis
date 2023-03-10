use heck::CamelCase;
use proc_macro2::{Ident, Span};
use quote::quote;

use core::filter::ast::*;
use core::filter::ptree::{PNode, PTree, Terminate};
use core::protocol;

use crate::util::binary_to_tokens;

// TODO: lots of opportunities to optimize further. But need to be careful about correctness
// example: collapse if statements at each header?
pub(crate) fn gen_packet_filter(
    ptree: &PTree,
    statics: &mut Vec<proc_macro2::TokenStream>,
) -> (proc_macro2::TokenStream, Vec<usize>) {
    if ptree.root.is_terminal {
        // only ethernet - no filter specified
        return (
            quote! {
                core::filter::FilterResult::MatchTerminal(0)
            },
            vec![],
        );
    }

    let name = "ethernet";
    let outer = Ident::new(name, Span::call_site());
    let outer_type = Ident::new(&outer.to_string().to_camel_case(), Span::call_site());

    let mut body: Vec<proc_macro2::TokenStream> = vec![];
    let mut pt_nodes = vec![];
    // dummy outer protocol for ethernet
    gen_packet_filter_util(
        &mut pt_nodes,
        &mut body,
        statics,
        &ptree.root,
        &protocol!("frame"),
    );
    #[cfg(all(feature = "retina", not(feature = "run")))]
    let packet_filter = quote! {
        if let Ok(#outer) = &core::protocols::packet::Packet::parse_to::<core::protocols::packet::#outer::#outer_type>(mbuf) {
            #( #body )*
        }
        return core::filter::FilterResult::NoMatch;
    };
    #[cfg(all(feature = "run", not(feature = "retina")))]
    let packet_filter = quote! {
        let cursor = core::protocols::packet::Cursor::new(mbuf.data());
        if let Ok(#outer) = core::protocols::packet::#outer::#outer_type::parse(cursor) {
            #( #body )*
        }
        return core::filter::FilterResult::NoMatch;
    };

    (packet_filter, pt_nodes)
}

fn gen_packet_filter_util(
    pt_nodes: &mut Vec<usize>,
    code: &mut Vec<proc_macro2::TokenStream>,
    statics: &mut Vec<proc_macro2::TokenStream>,
    node: &PNode,
    outer_protocol: &ProtocolName,
) {
    let mut first_unary = true;
    for child in node.children.iter().filter(|n| n.pred.on_packet()) {
        match &child.pred {
            Predicate::Unary { protocol } => {
                add_unary_pred(
                    pt_nodes,
                    code,
                    statics,
                    child,
                    node.pred.get_protocol(),
                    protocol,
                    first_unary,
                );
                first_unary = false;
            }
            Predicate::Binary {
                protocol,
                field,
                op,
                value,
            } => {
                add_binary_pred(
                    pt_nodes,
                    code,
                    statics,
                    child,
                    outer_protocol,
                    protocol,
                    field,
                    op,
                    value,
                );
            }
        }
    }
}

fn add_unary_pred(
    pt_nodes: &mut Vec<usize>,
    code: &mut Vec<proc_macro2::TokenStream>,
    statics: &mut Vec<proc_macro2::TokenStream>,
    node: &PNode,
    outer_protocol: &ProtocolName,
    protocol: &ProtocolName,
    first_unary: bool,
) {
    let outer = Ident::new(outer_protocol.name(), Span::call_site());
    let ident = Ident::new(protocol.name(), Span::call_site());
    let ident_type = Ident::new(&ident.to_string().to_camel_case(), Span::call_site());

    let mut body: Vec<proc_macro2::TokenStream> = vec![];
    gen_packet_filter_util(pt_nodes, &mut body, statics, node, outer_protocol);

    if matches!(node.terminates, Terminate::Packet) {
        pt_nodes.push(node.id);
        let idx_lit = syn::LitInt::new(&node.id.to_string(), Span::call_site());

        if node.is_terminal {
            body.push(quote! {
                return core::filter::FilterResult::MatchTerminal(#idx_lit);
            })
        } else {
            body.push(quote! {
                return core::filter::FilterResult::MatchNonTerminal(#idx_lit);
            });
        }
    }

    if first_unary {
        #[cfg(all(feature = "retina", not(feature = "run")))]
        code.push(quote! {
            if let Ok(#ident) = &core::protocols::packet::Packet::parse_to::<core::protocols::packet::#ident::#ident_type>(#outer) {
                #( #body )*
            }
        });
        #[cfg(all(feature = "run", not(feature = "retina")))]
        code.push(quote! {
            if let Ok(#ident) = core::protocols::packet::#ident::#ident_type::parse(#outer.cursor_payload()) {
                #( #body )*
            }
        });
    } else {
        #[cfg(all(feature = "retina", not(feature = "run")))]
        code.push(quote! {
            else if let Ok(#ident) = &core::protocols::packet::Packet::parse_to::<core::protocols::packet::#ident::#ident_type>(#outer) {
                #( #body )*
            }
        });
        #[cfg(all(feature = "run", not(feature = "retina")))]
        code.push(quote! {
            else if let Ok(#ident) = core::protocols::packet::#ident::#ident_type::parse(#outer.cursor_payload()) {
                #( #body )*
            }
        });
    }
}

#[allow(clippy::too_many_arguments)]
fn add_binary_pred(
    pt_nodes: &mut Vec<usize>,
    code: &mut Vec<proc_macro2::TokenStream>,
    statics: &mut Vec<proc_macro2::TokenStream>,
    node: &PNode,
    outer_protocol: &ProtocolName,
    protocol: &ProtocolName,
    field: &FieldName,
    op: &BinOp,
    value: &Value,
) {
    let mut body: Vec<proc_macro2::TokenStream> = vec![];
    gen_packet_filter_util(pt_nodes, &mut body, statics, node, outer_protocol);
    if matches!(node.terminates, Terminate::Packet) {
        pt_nodes.push(node.id);
        let idx_lit = syn::LitInt::new(&node.id.to_string(), Span::call_site());

        if node.is_terminal {
            body.push(quote! {
                return core::filter::FilterResult::MatchTerminal(#idx_lit);
            })
        } else {
            body.push(quote! {
                return core::filter::FilterResult::MatchNonTerminal(#idx_lit);
            });
        }
    }

    let pred_tokenstream = binary_to_tokens(protocol, field, op, value, statics);
    code.push(quote! {
        if #pred_tokenstream {
            #( #body )*
        }
    });
}
