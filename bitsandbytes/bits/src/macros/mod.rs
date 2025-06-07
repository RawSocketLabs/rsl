use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, Data, DataEnum, DataStruct, DeriveInput, Fields, Variant, Lit, Meta, NestedMeta};

/// Derive macro for creating a bitfield struct
#[proc_macro_derive(Bits, attributes(bits, bit))]
pub fn derive_bits(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = input.ident;
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();
    
    // Parse struct-level attributes
    let mut struct_start = 0;
    let mut struct_end = 0;
    let mut struct_mode = quote!(Access::ReadWrite);
    let mut struct_order = quote!(Endianness::Little);
    
    for attr in &input.attrs {
        if attr.path.is_ident("bits") {
            let meta = attr.parse_meta().unwrap();
            if let Meta::List(list) = meta {
                for nested in list.nested {
                    if let NestedMeta::Meta(Meta::NameValue(name_value)) = nested {
                        if name_value.path.is_ident("mode") {
                            if let Lit::Str(lit) = &name_value.lit {
                                match lit.value().as_str() {
                                    "r" => struct_mode = quote!(Access::Read),
                                    "w" => struct_mode = quote!(Access::Write),
                                    "rw" => struct_mode = quote!(Access::ReadWrite),
                                    _ => panic!("Invalid mode: {}", lit.value()),
                                }
                            }
                        } else if name_value.path.is_ident("order") {
                            if let Lit::Str(lit) = &name_value.lit {
                                match lit.value().as_str() {
                                    "Little" => struct_order = quote!(Endianness::Little),
                                    "Big" => struct_order = quote!(Endianness::Big),
                                    _ => panic!("Invalid order: {}", lit.value()),
                                }
                            }
                        }
                    } else if let NestedMeta::Lit(Lit::Int(lit)) = nested {
                        if struct_start == 0 {
                            struct_start = lit.base10_parse::<usize>().unwrap();
                        } else {
                            struct_end = lit.base10_parse::<usize>().unwrap();
                        }
                    }
                }
            }
        }
    }
    
    let fields = match input.data {
        Data::Struct(DataStruct { fields: Fields::Named(fields), .. }) => {
            fields.named.iter().map(|field| {
                let field_name = &field.ident;
                let field_type = &field.ty;
                let attrs = &field.attrs;
                
                // Parse field attributes
                let mut offset = None;
                let mut width = None;
                let mut mode = struct_mode.clone();
                let mut order = struct_order.clone();
                
                for attr in attrs {
                    if attr.path.is_ident("bit") {
                        let meta = attr.parse_meta().unwrap();
                        if let Meta::List(list) = meta {
                            for nested in list.nested {
                                if let NestedMeta::Meta(Meta::NameValue(name_value)) = nested {
                                    if name_value.path.is_ident("mode") {
                                        if let Lit::Str(lit) = &name_value.lit {
                                            match lit.value().as_str() {
                                                "r" => mode = quote!(Access::Read),
                                                "w" => mode = quote!(Access::Write),
                                                "rw" => mode = quote!(Access::ReadWrite),
                                                _ => panic!("Invalid mode: {}", lit.value()),
                                            }
                                        }
                                    }
                                } else if let NestedMeta::Lit(Lit::Int(lit)) = nested {
                                    offset = Some(lit.base10_parse::<usize>().unwrap());
                                    width = Some(1);
                                }
                            }
                        }
                    } else if attr.path.is_ident("bits") {
                        let meta = attr.parse_meta().unwrap();
                        if let Meta::List(list) = meta {
                            for nested in list.nested {
                                if let NestedMeta::Meta(Meta::NameValue(name_value)) = nested {
                                    if name_value.path.is_ident("mode") {
                                        if let Lit::Str(lit) = &name_value.lit {
                                            match lit.value().as_str() {
                                                "r" => mode = quote!(Access::Read),
                                                "w" => mode = quote!(Access::Write),
                                                "rw" => mode = quote!(Access::ReadWrite),
                                                _ => panic!("Invalid mode: {}", lit.value()),
                                            }
                                        }
                                    } else if name_value.path.is_ident("order") {
                                        if let Lit::Str(lit) = &name_value.lit {
                                            match lit.value().as_str() {
                                                "Little" => order = quote!(Endianness::Little),
                                                "Big" => order = quote!(Endianness::Big),
                                                _ => panic!("Invalid order: {}", lit.value()),
                                            }
                                        }
                                    }
                                } else if let NestedMeta::Lit(Lit::Int(lit)) = nested {
                                    if offset.is_none() {
                                        offset = Some(lit.base10_parse::<usize>().unwrap());
                                    } else {
                                        width = Some(lit.base10_parse::<usize>().unwrap() - offset.unwrap() + 1);
                                    }
                                }
                            }
                        }
                    }
                }
                
                let offset = offset.expect("Field must have an offset");
                let width = width.expect("Field must have a width");
                
                quote! {
                    .field(stringify!(#field_name), #offset, #width, #mode)
                }
            }).collect::<Vec<_>>()
        },
        _ => panic!("Bits can only be derived for structs with named fields"),
    };
    
    let expanded = quote! {
        impl #impl_generics #name #ty_generics #where_clause {
            pub fn new() -> Self {
                let bitfield = Bitfield::builder(#struct_order, 0)
                    #(#fields)*
                    .build();
                Self { bitfield }
            }
            
            pub fn get(&self, name: &str) -> Option<u8> {
                self.bitfield.get(name)
            }
            
            pub fn set(&mut self, name: &str, value: u8) -> Option<()> {
                self.bitfield.set(name, value)
            }
        }
    };
    
    TokenStream::from(expanded)
}

/// Derive macro for creating a bitfield enum
#[proc_macro_derive(BitEnum)]
pub fn derive_bit_enum(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = input.ident;
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();
    
    let variants = match input.data {
        Data::Enum(DataEnum { variants, .. }) => {
            variants.iter().map(|variant| {
                let variant_name = &variant.ident;
                let variant_value = match &variant.discriminant {
                    Some((_, expr)) => quote!(#expr),
                    None => quote!(0),
                };
                quote! {
                    #name::#variant_name => #variant_value
                }
            }).collect::<Vec<_>>()
        },
        _ => panic!("BitEnum can only be derived for enums"),
    };
    
    let variant_names = match input.data {
        Data::Enum(DataEnum { variants, .. }) => {
            variants.iter().map(|variant| {
                let variant_name = &variant.ident;
                quote! {
                    stringify!(#variant_name)
                }
            }).collect::<Vec<_>>()
        },
        _ => panic!("BitEnum can only be derived for enums"),
    };
    
    let expanded = quote! {
        impl #impl_generics BitValue for #name #ty_generics #where_clause {
            type IntType = u8;
            
            fn from_int(value: Self::IntType) -> Self {
                match value {
                    #(#variants,)*
                    _ => panic!("Invalid value for enum"),
                }
            }
            
            fn to_int(self) -> Self::IntType {
                match self {
                    #(#variants,)*
                }
            }
            
            fn bit_width() -> usize {
                8
            }
        }
        
        impl #impl_generics BitEnum for #name #ty_generics #where_clause {
            fn name(&self) -> &'static str {
                match self {
                    #(
                        #name::#variant_name => #variant_names,
                    )*
                }
            }
            
            fn all_values() -> &'static [Self] {
                static VALUES: &[#name] = &[
                    #(#name::#variant_name,)*
                ];
                VALUES
            }
        }
    };
    
    TokenStream::from(expanded)
} 