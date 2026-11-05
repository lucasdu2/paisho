use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::{
    Error, Expr, FnArg, GenericArgument, ImplItem, ItemImpl, Lit, Meta, Pat, PathArguments,
    ReturnType, Type, parse_macro_input, spanned::Spanned,
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
    // So the tools can be on a type with lifetimes, such as a view borrowing environments.
    let generics = input.generics.clone();
    let (impl_generics, _, where_clause) = generics.split_for_impl();
    let Type::Path(self_path) = &*self_ty else {
        return Err(Error::new(self_ty.span(), "#[tools] needs a named type"));
    };
    let self_name = self_path.path.segments.last().unwrap().ident.clone();

    let mut arg_structs = Vec::new();
    let mut arms = Vec::new();
    let mut schemas = Vec::new();
    let mut names = Vec::new();

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
        // `T` in `Result<T, String>`.
        let Some(ok_ty) = result_ok_type(ret) else {
            return Err(Error::new(
                ret.span(),
                "tools must return Result<T, String>",
            ));
        };
        // Tools returning `()` have no structured content, so no output schema.
        let set_output_schema = if matches!(ok_ty, Type::Tuple(t) if t.elems.is_empty()) {
            quote! {}
        } else {
            quote! { schema["output_schema"] = crate::suites::output_schema::<#ok_ty>(); }
        };

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
            // schemars turns into the property's description, and `#[serde(...)]` and
            // `#[schemars(...)]` (such as `#[serde(default)]`) move to the field. They are
            // stripped from the method.
            let mut field_docs = Vec::new();
            pat_ty.attrs.retain(|attr| match &attr.meta {
                Meta::NameValue(nv) if nv.path.is_ident("describe") => {
                    let value = &nv.value;
                    field_docs.push(quote! { #[doc = #value] });
                    false
                }
                meta if meta.path().is_ident("serde") || meta.path().is_ident("schemars") => {
                    field_docs.push(quote! { #attr });
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
                let args: #args_ty = match ::serde_json::from_value(::serde_json::Value::Object(call.arguments.clone())) {
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
            let mut schema = ::serde_json::json!({
                "name": #name_str,
                "description": #description,
                "input_schema": input_schema,
            });
            #set_output_schema
            schema
        }});
        names.push(name_str.clone());
    }

    Ok(quote! {
        #input
        #(#arg_structs)*
        impl #impl_generics #self_ty #where_clause {
            /// Names of every tool, known at compile time.
            pub const TOOL_NAMES: &'static [&'static str] = &[#(#names),*];

            /// JSON descriptions of every tool, in the format LLM tool-use APIs expect.
            pub fn tool_schemas() -> ::std::vec::Vec<::serde_json::Value> {
                vec![#(#schemas),*]
            }

            /// JSON description of one tool, or `None` if no tool has that name.
            pub fn tool_schema(name: &str) -> Option<::serde_json::Value> {
                match name {
                    #(#names => Some(#schemas),)*
                    _ => None,
                }
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

        impl #impl_generics crate::environments::Environment for #self_ty #where_clause {
            fn tool_schemas(&self) -> ::std::vec::Vec<::serde_json::Value> {
                Self::tool_schemas()
            }

            fn tool_schema(&self, name: &str) -> Option<::serde_json::Value> {
                Self::tool_schema(name)
            }

            fn dispatch(
                &mut self,
                call: &crate::suites::ToolCall,
            ) -> Result<crate::suites::ToolResult, crate::suites::ToolError> {
                Self::dispatch(self, call)
            }
        }
    })
}

/// The `T` of a `Result<T, E>` type, or `None` if `ty` isn't a `Result`.
fn result_ok_type(ty: &Type) -> Option<&Type> {
    let Type::Path(p) = ty else { return None };
    let last = p.path.segments.last()?;
    if last.ident != "Result" {
        return None;
    }
    let PathArguments::AngleBracketed(args) = &last.arguments else {
        return None;
    };
    match args.args.first()? {
        GenericArgument::Type(ok_ty) => Some(ok_ty),
        _ => None,
    }
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
