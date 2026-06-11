//! The `#[wire]` attribute macro.
//!
//! `#[wire]` folds the protocol-header triad — binrw codec + builder +
//! collapsed bit-groups + derived fields + soundness — into one attribute. It is
//! *sugar that expands to the existing primitives*: it rewrites the struct into a
//! `#[binrw]` struct (so the **entire** binrw attribute surface stays available
//! as an escape hatch), generates a private `#[bitfield]` per bit-group, and
//! generates a `BitsBuilder`-style builder. See `bits/DESIGN.md` §9.
//!
//! ## What each native feature lowers to
//!
//! - `group(a, b, c => u16)` (struct-level, order-sensitive): a private
//!   `#[bitfield(u16)]` packs the members; the wire field is `#[br(temp)]`
//!   (read the packed word into a local) + `#[bw(calc = Grp::new().with_a(self.a)…)]`,
//!   and each member becomes a stored `#[br(calc = grp.a())] #[bw(ignore)]` field.
//!   The matched read/write pair is generated together, so the two directions
//!   cannot drift (the binrw #47 hazard).
//! - `#[update(expr)]`: `#[br(temp)] #[bw(calc = expr)]` — not stored, recomputed
//!   on every write.
//! - `#[builder_only]` / `#[builder_only(default = expr)]`: `#[br(calc = default)]
//!   #[bw(ignore)]` — a builder/struct field that is not on the wire.
//! - `validate = path`: a `check_soundness` builder-only flag (auto-created,
//!   default `true`) gates a generated `validate(&self)` method; `build()` calls
//!   it. The **parser is left permissive** (it never rejects representable input),
//!   per the workspace's dual-use rule — validation is a construction-time /
//!   opt-in concern, not a parser concern.

use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote};
use syn::parse::{Parse, ParseStream};
use syn::{parenthesized, parse2, parse_quote, Attribute, Expr, Fields, Ident, ItemStruct, Path, Token, Type, Visibility};

use crate::builder::{self, BField, BuildKind, FieldDefault};

/// Wire byte order for the whole message.
#[derive(Clone, Copy)]
enum Endian {
    Big,
    Little,
}

/// One `group(a, b, … => uN)` clause.
struct GroupDecl {
    members: Vec<Ident>,
    backing: Type,
}

/// Parsed `#[wire(...)]` arguments.
struct MessageArgs {
    endian: Endian,
    groups: Vec<GroupDecl>,
    validate: Option<Path>,
    no_builder: bool,
}

impl Parse for MessageArgs {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let mut endian = None;
        let mut groups = Vec::new();
        let mut validate = None;
        let mut no_builder = false;

        while !input.is_empty() {
            let key: Ident = input.parse()?;
            match key.to_string().as_str() {
                "big" => endian = Some(Endian::Big),
                "little" => endian = Some(Endian::Little),
                "no_builder" => no_builder = true,
                "validate" => {
                    input.parse::<Token![=]>()?;
                    validate = Some(input.parse()?);
                }
                "group" => {
                    let content;
                    parenthesized!(content in input);
                    let mut members = Vec::new();
                    loop {
                        members.push(content.parse::<Ident>()?);
                        if content.peek(Token![=>]) {
                            break;
                        }
                        content.parse::<Token![,]>()?;
                    }
                    content.parse::<Token![=>]>()?;
                    let backing: Type = content.parse()?;
                    groups.push(GroupDecl { members, backing });
                }
                other => {
                    return Err(syn::Error::new(
                        key.span(),
                        format!(
                            "unknown `#[wire]` argument `{other}` (expected `big`, \
                             `little`, `group(a, b => uN)`, `validate = path`, or `no_builder`)"
                        ),
                    ));
                }
            }
            if !input.is_empty() {
                input.parse::<Token![,]>()?;
            }
        }

        Ok(MessageArgs {
            endian: endian.unwrap_or(Endian::Big),
            groups,
            validate,
            no_builder,
        })
    }
}

/// Per-field classification after our attributes are read.
enum Kind {
    /// Ordinary field: passes through to binrw and the builder unchanged.
    Plain,
    /// `#[update(expr)]`: derived on write, temp on read, not stored.
    Update(Expr),
    /// `#[builder_only]` / `#[builder_only(default = expr)]`: off the wire.
    BuilderOnly(Option<Expr>),
    /// A member of group `index`.
    GroupMember(usize),
}

/// One analyzed field.
struct AField {
    ident: Ident,
    ty: Type,
    vis: Visibility,
    kind: Kind,
    builder_default: FieldDefault,
    /// Attributes to forward (doc/cfg + binrw `#[br]/#[bw]/#[brw]` for plain
    /// fields; our markers and `#[builder]` are stripped).
    forward: Vec<Attribute>,
}

/// Pre-computed layout for one group.
struct GroupInfo {
    /// Field indices of the members, in order.
    indices: Vec<usize>,
    /// `(ident, type)` of each member, in order.
    members: Vec<(Ident, Type)>,
    backing: Type,
    ty_name: Ident,
    local: Ident,
}

fn is_marker_attr(a: &Attribute) -> bool {
    a.path().is_ident("update") || a.path().is_ident("builder_only") || a.path().is_ident("builder")
}

fn is_binrw_attr(a: &Attribute) -> bool {
    a.path().is_ident("br") || a.path().is_ident("bw") || a.path().is_ident("brw")
}

pub fn expand(attr: TokenStream, item: TokenStream) -> TokenStream {
    match expand_inner(attr.into(), item.into()) {
        Ok(ts) => ts.into(),
        Err(e) => e.to_compile_error().into(),
    }
}

fn expand_inner(attr: TokenStream2, item: TokenStream2) -> syn::Result<TokenStream2> {
    let args: MessageArgs = parse2(attr)?;
    let input: ItemStruct = parse2(item)?;

    let named = match &input.fields {
        Fields::Named(n) => n,
        _ => {
            return Err(syn::Error::new_spanned(
                &input.ident,
                "#[wire] requires a struct with named fields",
            ))
        }
    };
    let name = &input.ident;
    let vis = &input.vis;
    let struct_attrs = &input.attrs;

    let field_idents: Vec<Ident> = named
        .named
        .iter()
        .map(|f| f.ident.clone().expect("named field"))
        .collect();
    let position = |id: &Ident| field_idents.iter().position(|f| f == id);

    // ---- validate groups: members must exist, be consecutive and in order ----
    let mut member_group: Vec<Option<usize>> = vec![None; field_idents.len()];
    let mut groups: Vec<GroupInfo> = Vec::new();
    for (gi, g) in args.groups.iter().enumerate() {
        let mut indices = Vec::new();
        let mut members = Vec::new();
        let mut last: Option<usize> = None;
        for m in &g.members {
            let idx = position(m).ok_or_else(|| {
                syn::Error::new(m.span(), format!("group field `{m}` is not a field of this struct"))
            })?;
            if member_group[idx].is_some() {
                return Err(syn::Error::new(m.span(), format!("field `{m}` appears in more than one group")));
            }
            if let Some(prev) = last {
                if idx != prev + 1 {
                    return Err(syn::Error::new(
                        m.span(),
                        format!(
                            "group fields must be consecutive and in declared order; \
                             `{m}` does not immediately follow the previous group member \
                             in the struct body"
                        ),
                    ));
                }
            }
            member_group[idx] = Some(gi);
            last = Some(idx);
            indices.push(idx);
            members.push((m.clone(), named.named[idx].ty.clone()));
        }
        groups.push(GroupInfo {
            indices,
            members,
            backing: g.backing.clone(),
            ty_name: format_ident!("__{}Group{}", name, gi),
            local: format_ident!("__group{gi}"),
        });
    }

    // ---- analyze each field ----
    let mut afields: Vec<AField> = Vec::new();
    let mut has_check_soundness = false;
    for (idx, f) in named.named.iter().enumerate() {
        let ident = f.ident.clone().expect("named field");
        if ident == "check_soundness" {
            has_check_soundness = true;
        }

        let mut update: Option<Expr> = None;
        let mut builder_only: Option<Option<Expr>> = None;
        let mut builder_default = FieldDefault::Required;
        let mut forward: Vec<Attribute> = Vec::new();
        let mut has_binrw = false;

        for a in &f.attrs {
            if a.path().is_ident("update") {
                update = Some(a.parse_args()?);
            } else if a.path().is_ident("builder_only") {
                if matches!(&a.meta, syn::Meta::Path(_)) {
                    builder_only = Some(None);
                } else {
                    let mut def = None;
                    a.parse_nested_meta(|meta| {
                        if meta.path.is_ident("default") {
                            def = Some(meta.value()?.parse::<Expr>()?);
                            Ok(())
                        } else {
                            Err(meta.error("expected `default = <expr>`"))
                        }
                    })?;
                    builder_only = Some(def);
                }
            } else if let Some(d) = builder::parse_builder_attr(a)? {
                builder_default = d;
            } else {
                if is_binrw_attr(a) {
                    has_binrw = true;
                }
                if !is_marker_attr(a) {
                    forward.push(a.clone());
                }
            }
        }

        let in_group = member_group[idx];
        let count =
            in_group.is_some() as u8 + update.is_some() as u8 + builder_only.is_some() as u8;
        if count > 1 {
            return Err(syn::Error::new(
                ident.span(),
                "a field cannot combine group membership, `#[update]`, and `#[builder_only]`",
            ));
        }

        let kind = if let Some(gi) = in_group {
            Kind::GroupMember(gi)
        } else if let Some(e) = update {
            Kind::Update(e)
        } else if let Some(d) = builder_only {
            // `#[builder_only(default = expr)]` makes the field optional in the
            // builder too (with the same default it gets on read).
            if let Some(e) = &d {
                builder_default = FieldDefault::DefaultExpr(e.clone());
            }
            Kind::BuilderOnly(d)
        } else {
            Kind::Plain
        };

        // Non-plain fields generate their own br/bw; a user-written one collides.
        if !matches!(kind, Kind::Plain) && has_binrw {
            return Err(syn::Error::new(
                ident.span(),
                "group / `#[update]` / `#[builder_only]` fields generate their own \
                 binrw attributes; remove the explicit `#[br]`/`#[bw]`/`#[brw]`",
            ));
        }

        // For non-plain fields, forward only inert attrs (doc/cfg), never binrw.
        if !matches!(kind, Kind::Plain) {
            forward.retain(|a| !is_binrw_attr(a));
        }

        afields.push(AField {
            ident,
            ty: f.ty.clone(),
            vis: f.vis.clone(),
            kind,
            builder_default,
            forward,
        });
    }

    // ---- emit transformed binrw fields (inserting group temps) ----
    let mut out_fields: Vec<TokenStream2> = Vec::new();
    for (idx, af) in afields.iter().enumerate() {
        let AField { ident, ty, vis, forward, .. } = af;
        match &af.kind {
            Kind::Plain => out_fields.push(quote! {
                #(#forward)*
                #vis #ident: #ty,
            }),
            Kind::Update(expr) => out_fields.push(quote! {
                #(#forward)*
                #[br(temp)]
                #[bw(calc = #expr)]
                #ident: #ty,
            }),
            Kind::BuilderOnly(def) => {
                let default: Expr =
                    def.clone().unwrap_or_else(|| parse_quote!(::core::default::Default::default()));
                out_fields.push(quote! {
                    #(#forward)*
                    #[br(calc = #default)]
                    #[bw(ignore)]
                    #vis #ident: #ty,
                });
            }
            Kind::GroupMember(gi) => {
                let g = &groups[*gi];
                // Insert the packed temp field just before the first member.
                if g.indices[0] == idx {
                    let gty = &g.ty_name;
                    let local = &g.local;
                    let withs = g.members.iter().map(|(m, _)| {
                        let w = format_ident!("with_{m}");
                        quote!(.#w(self.#m))
                    });
                    out_fields.push(quote! {
                        #[br(temp)]
                        #[bw(calc = #gty::new() #(#withs)*)]
                        #local: #gty,
                    });
                }
                let local = &g.local;
                out_fields.push(quote! {
                    #(#forward)*
                    #[br(calc = #local.#ident())]
                    #[bw(ignore)]
                    #vis #ident: #ty,
                });
            }
        }
    }

    // ---- auto check_soundness flag when validating ----
    let want_soundness_field = args.validate.is_some() && !has_check_soundness;
    if want_soundness_field {
        out_fields.push(quote! {
            /// Whether [`validate`](Self::validate) runs (dual-use: set `false`
            /// via the builder to construct deliberately malformed messages).
            #[br(calc = true)]
            #[bw(ignore)]
            pub check_soundness: bool,
        });
    }

    // ---- group bitfields ----
    let be_le = match args.endian {
        Endian::Big => quote!(be),
        Endian::Little => quote!(le),
    };
    let group_defs = groups.iter().map(|g| {
        let gty = &g.ty_name;
        let backing = &g.backing;
        let (mids, mtys): (Vec<_>, Vec<_>) = g.members.iter().map(|(i, t)| (i, t)).unzip();
        quote! {
            #[::bits::bitfield(#backing, bits = msb, bytes = #be_le)]
            #[derive(::core::clone::Clone, ::core::marker::Copy)]
            struct #gty {
                #( #mids: #mtys, )*
            }
        }
    });

    // ---- builder fields (everything except `#[update]`; plus auto flag) ----
    let mut bfields: Vec<BField> = afields
        .iter()
        .filter(|af| !matches!(af.kind, Kind::Update(_)))
        .map(|af| BField {
            ident: af.ident.clone(),
            ty: af.ty.clone(),
            default: af.builder_default.clone(),
        })
        .collect();
    if want_soundness_field {
        bfields.push(BField {
            ident: format_ident!("check_soundness"),
            ty: parse_quote!(bool),
            default: FieldDefault::DefaultExpr(parse_quote!(true)),
        });
    }

    // ---- validate() method + build() hook ----
    let (validate_impl, post_build) = if let Some(path) = &args.validate {
        let v = quote! {
            impl #name {
                /// Runs the soundness validator when `check_soundness` is set.
                ///
                /// Called automatically by `build()`. Call it yourself after
                /// parsing if you want to check a received message (the parser
                /// itself stays permissive and never rejects representable input).
                #vis fn validate(&self) -> ::core::result::Result<(), ::bits::BuilderError> {
                    if self.check_soundness {
                        (#path)(self).map_err(|__e| ::bits::BuilderError::invalid(__e.to_string()))?;
                    }
                    ::core::result::Result::Ok(())
                }
            }
        };
        (v, Some(quote!(__value.validate()?;)))
    } else {
        (quote!(), None)
    };

    let builder_tokens = if args.no_builder {
        quote!()
    } else {
        builder::generate(name, vis, &bfields, BuildKind::Plain, post_build.as_ref())
    };

    let endian_kw = match args.endian {
        Endian::Big => quote!(big),
        Endian::Little => quote!(little),
    };

    Ok(quote! {
        #(#group_defs)*

        #[::binrw::binrw]
        #[brw(#endian_kw)]
        #(#struct_attrs)*
        #vis struct #name {
            #(#out_fields)*
        }

        #validate_impl
        #builder_tokens
    })
}
