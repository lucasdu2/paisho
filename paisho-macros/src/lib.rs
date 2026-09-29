use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::{
    parse_macro_input, spanned::Spanned, Error, Expr, FnArg, ImplItem, ItemImpl, Lit, Meta, Pat,
    ReturnType, Type,
};

/// Turns every method in an `impl` block into a tool callable through `dispatch`.
#[proc_macro_attribute]
pub fn tools(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as ItemImpl);
    match expand(input) {
        Ok(tokens) => tokens.into(),
        Err(err) => err.to_compile_error().into(),
    }
}

fn expand(mut input: ItemImpl) -> syn::Result<proc_macro2::TokenStream> {
    let self_ty = input.self_ty.clone();
    let Type::Path(self_path) = &*self_ty else {
        return Err(Error::new(self_ty.span(), "#[tools] needs a named type"));
    };
    let self_name = self_path.path.segments.last().unwrap().ident.clone();

    let mut arg_structs = Vec::new();
    let mut arms = Vec::new();
    let mut schemas = Vec::new();

    for item in &mut input.items {
        let ImplItem::Fn(method) = item else { continue };
        let name = method.sig.ident.clone();
        let name_str = name.to_string();

        if method.sig.receiver().is_none() {
            return Err(Error::new(
                method.sig.span(),
                "tools must take &self or &mut self",
            ));
        }
        let ReturnType::Type(_, ret) = &method.sig.output else {
            return Err(Error::new(
                method.sig.span(),
                "tools must return Result<T, String>",
            ));
        };
        let returns_result = matches!(&**ret, Type::Path(p)
            if p.path.segments.last().is_some_and(|s| s.ident == "Result"));
        if !returns_result {
            return Err(Error::new(
                ret.span(),
                "tools must return Result<T, String>",
            ));
        }

        let description = doc_string(&method.attrs);

        // One field per argument; `&str` arguments are stored as `String`.
        let mut fields = Vec::new();
        let mut call_args = Vec::new();
        for arg in method.sig.inputs.iter_mut().skip(1) {
            let FnArg::Typed(pat_ty) = arg else {
                unreachable!()
            };
            let Pat::Ident(pat_ident) = &*pat_ty.pat else {
                return Err(Error::new(
                    pat_ty.pat.span(),
                    "tool arguments must be plain identifiers",
                ));
            };
            let field = format_ident!("{}", pat_ident.ident.to_string().trim_start_matches('_'));

            // `#[describe = "..."]` on an argument becomes that field's doc comment, which
            // schemars turns into the property's description. It is stripped from the method.
            let mut field_docs = Vec::new();
            pat_ty.attrs.retain(|attr| match &attr.meta {
                Meta::NameValue(nv) if nv.path.is_ident("describe") => {
                    let value = &nv.value;
                    field_docs.push(quote! { #[doc = #value] });
                    false
                }
                _ => true,
            });

            let is_str_ref = matches!(&*pat_ty.ty, Type::Reference(r)
                if matches!(&*r.elem, Type::Path(p) if p.path.is_ident("str")));
            let field_ty = if is_str_ref {
                quote! { ::std::string::String }
            } else {
                let ty = &pat_ty.ty;
                quote! { #ty }
            };
            fields.push(quote! { #(#field_docs)* #field: #field_ty });
            call_args.push(if is_str_ref {
                quote! { &args.#field }
            } else {
                quote! { args.#field }
            });
        }

        let args_ty = format_ident!("__{}_{}_args", self_name, name);
        arg_structs.push(quote! {
            #[allow(non_camel_case_types)]
            #[derive(::serde::Deserialize, ::schemars::JsonSchema)]
            #[serde(deny_unknown_fields)]
            struct #args_ty { #(#fields),* }
        });
        arms.push(quote! {
            #name_str => {
                let args: #args_ty = match ::serde_json::from_value(call.arguments.clone()) {
                    Ok(args) => args,
                    Err(e) => return Ok(crate::suites::ToolResult::error(format!("{}: {}", #name_str, e))),
                };
                Ok(match self.#name(#(#call_args),*) {
                    Ok(out) => match ::serde_json::to_value(out) {
                        Ok(value) => crate::suites::ToolResult::success(value),
                        Err(e) => crate::suites::ToolResult::error(
                            format!("{}: could not serialize result: {}", #name_str, e)
                        ),
                    },
                    Err(e) => crate::suites::ToolResult::error(e),
                })
            }
        });
        schemas.push(quote! {{
            let mut input_schema = ::serde_json::to_value(::schemars::schema_for!(#args_ty)).unwrap();
            if let ::serde_json::Value::Object(map) = &mut input_schema {
                map.remove("$schema");
                map.remove("title");
            }
            ::serde_json::json!({
                "name": #name_str,
                "description": #description,
                "input_schema": input_schema,
            })
        }});
    }

    Ok(quote! {
        #input
        #(#arg_structs)*
        impl #self_ty {
            /// JSON descriptions of every tool, in the format LLM tool-use APIs expect.
            pub fn tool_schemas() -> ::std::vec::Vec<::serde_json::Value> {
                vec![#(#schemas),*]
            }

            pub fn dispatch(
                &mut self,
                call: &crate::suites::ToolCall,
            ) -> Result<crate::suites::ToolResult, crate::suites::ToolError> {
                match call.name.as_str() {
                    #(#arms)*
                    other => Err(crate::suites::ToolError::UnknownTool(other.to_string())),
                }
            }
        }
    })
}

/// Joins the `///` doc comment lines on an item into one string.
fn doc_string(attrs: &[syn::Attribute]) -> String {
    attrs
        .iter()
        .filter_map(|attr| match &attr.meta {
            Meta::NameValue(nv) if nv.path.is_ident("doc") => match &nv.value {
                Expr::Lit(e) => match &e.lit {
                    Lit::Str(s) => Some(s.value().trim().to_string()),
                    _ => None,
                },
                _ => None,
            },
            _ => None,
        })
        .collect::<Vec<_>>()
        .join(" ")
}
