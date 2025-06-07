use std::fmt;
use std::marker::PhantomData;
use std::ops::{BitAnd, BitOr, Not};

pub mod macros;
pub mod types;

pub use types::*;

/// Represents the endianness of a bitfield
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Endianness {
    /// Little endian byte order
    Little,
    /// Big endian byte order
    Big,
}

/// Represents the access permissions for a field
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Access {
    /// Read-only access
    Read,
    /// Write-only access
    Write,
    /// Read-write access
    ReadWrite,
}

/// A trait for types that can be used as bitfield values
pub trait BitValue: 
    Copy + 
    BitAnd<Output = Self> + 
    BitOr<Output = Self> + 
    Not<Output = Self> + 
    fmt::Debug + 
    fmt::Display + 
    PartialEq + 
    Eq
{
    /// The underlying integer type
    type IntType: Copy + BitAnd<Output = Self::IntType> + BitOr<Output = Self::IntType> + Not<Output = Self::IntType>;
    
    /// Convert from the underlying integer type
    fn from_int(value: Self::IntType) -> Self;
    
    /// Convert to the underlying integer type
    fn to_int(self) -> Self::IntType;
    
    /// Get the bit width of this type
    fn bit_width() -> usize;
}

// Implement BitValue for common integer types
macro_rules! impl_bit_value {
    ($($t:ty),*) => {
        $(
            impl BitValue for $t {
                type IntType = $t;
                
                fn from_int(value: Self::IntType) -> Self {
                    value
                }
                
                fn to_int(self) -> Self::IntType {
                    self
                }
                
                fn bit_width() -> usize {
                    std::mem::size_of::<$t>() * 8
                }
            }
        )*
    };
}

impl_bit_value!(u8, u16, u32, u64, u128);

/// A trait for types that can be used as bitfield enums
pub trait BitEnum: BitValue {
    /// Get the name of this enum variant
    fn name(&self) -> &'static str;
    
    /// Get all possible values of this enum
    fn all_values() -> &'static [Self];
    
    /// Get the bit width required to represent all variants
    fn required_bits() -> usize {
        let values = Self::all_values();
        if values.is_empty() {
            return 0;
        }
        let max_value = values.iter()
            .map(|v| v.to_int())
            .max()
            .unwrap();
        (max_value as f64).log2().ceil() as usize
    }
}

/// A bitfield field definition
#[derive(Debug, Clone, Copy)]
pub struct Field<T: BitValue> {
    /// The name of the field
    pub name: &'static str,
    /// The bit offset from the start of the struct
    pub offset: usize,
    /// The bit width of the field
    pub width: usize,
    /// The access permissions for the field
    pub access: Access,
    /// The type of the field
    _type: PhantomData<T>,
}

impl<T: BitValue> Field<T> {
    /// Create a new field
    pub fn new(name: &'static str, offset: usize, width: usize, access: Access) -> Self {
        assert!(width <= T::bit_width(), "Field width exceeds type width");
        Self {
            name,
            offset,
            width,
            access,
            _type: PhantomData,
        }
    }
    
    /// Get the bit mask for this field
    pub fn mask(&self) -> T::IntType {
        let mut mask = T::IntType::from(0);
        for i in 0..self.width {
            mask = mask | (T::IntType::from(1) << i);
        }
        mask << self.offset
    }
    
    /// Check if this field overlaps with another field
    pub fn overlaps_with(&self, other: &Field<T>) -> bool {
        let self_start = self.offset;
        let self_end = self.offset + self.width - 1;
        let other_start = other.offset;
        let other_end = other.offset + other.width - 1;
        
        (self_start <= other_end) && (other_start <= self_end)
    }
}

/// A bitfield struct builder
pub struct BitfieldBuilder<T: BitValue> {
    fields: Vec<Field<T>>,
    endianness: Endianness,
    default_value: T,
    _type: PhantomData<T>,
}

impl<T: BitValue> BitfieldBuilder<T> {
    /// Create a new bitfield builder
    pub fn new(endianness: Endianness, default_value: T) -> Self {
        Self {
            fields: Vec::new(),
            endianness,
            default_value,
            _type: PhantomData,
        }
    }
    
    /// Add a field to the bitfield
    pub fn field(mut self, name: &'static str, offset: usize, width: usize, access: Access) -> Self {
        let new_field = Field::new(name, offset, width, access);
        
        // Check for overlaps with existing fields
        for field in &self.fields {
            if new_field.overlaps_with(field) {
                panic!("Field {} overlaps with field {}", name, field.name);
            }
        }
        
        self.fields.push(new_field);
        self
    }
    
    /// Build the bitfield struct
    pub fn build(self) -> Bitfield<T> {
        Bitfield {
            value: self.default_value,
            fields: self.fields,
            endianness: self.endianness,
            _type: PhantomData,
        }
    }
}

/// A bitfield struct
#[derive(Clone, Copy)]
pub struct Bitfield<T: BitValue> {
    value: T,
    fields: Vec<Field<T>>,
    endianness: Endianness,
    _type: PhantomData<T>,
}

impl<T: BitValue> Bitfield<T> {
    /// Create a new bitfield builder
    pub fn builder(endianness: Endianness, default_value: T) -> BitfieldBuilder<T> {
        BitfieldBuilder::new(endianness, default_value)
    }
    
    /// Get the value of a field
    pub fn get(&self, name: &str) -> Option<T> {
        self.fields.iter()
            .find(|f| f.name == name)
            .filter(|f| matches!(f.access, Access::Read | Access::ReadWrite))
            .map(|field| {
                let mask = field.mask();
                let value = (self.value.to_int() & mask) >> field.offset;
                T::from_int(value)
            })
    }
    
    /// Set the value of a field
    pub fn set(&mut self, name: &str, value: T) -> Option<()> {
        self.fields.iter()
            .find(|f| f.name == name)
            .filter(|f| matches!(f.access, Access::Write | Access::ReadWrite))
            .map(|field| {
                let mask = field.mask();
                let value = (value.to_int() << field.offset) & mask;
                let current = self.value.to_int() & !mask;
                self.value = T::from_int(current | value);
            })
    }
    
    /// Get the raw value of the bitfield
    pub fn raw_value(&self) -> T {
        self.value
    }
    
    /// Set the raw value of the bitfield
    pub fn set_raw_value(&mut self, value: T) {
        self.value = value;
    }
    
    /// Get a nested bitfield for a field
    pub fn nested<U: BitValue>(&self, name: &str) -> Option<Bitfield<U>> {
        self.fields.iter()
            .find(|f| f.name == name)
            .filter(|f| matches!(f.access, Access::Read | Access::ReadWrite))
            .map(|field| {
                let mask = field.mask();
                let value = (self.value.to_int() & mask) >> field.offset;
                Bitfield::<U>::builder(self.endianness, U::from_int(value as U::IntType))
                    .build()
            })
    }
    
    /// Set a nested bitfield for a field
    pub fn set_nested<U: BitValue>(&mut self, name: &str, nested: Bitfield<U>) -> Option<()> {
        self.fields.iter()
            .find(|f| f.name == name)
            .filter(|f| matches!(f.access, Access::Write | Access::ReadWrite))
            .map(|field| {
                let mask = field.mask();
                let value = (nested.raw_value().to_int() as T::IntType) << field.offset;
                let current = self.value.to_int() & !mask;
                self.value = T::from_int(current | value);
            })
    }
}

impl<T: BitValue> fmt::Debug for Bitfield<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Bitfield")
            .field("endianness", &self.endianness)
            .field("fields", &self.fields.iter()
                .filter(|f| matches!(f.access, Access::Read | Access::ReadWrite))
                .map(|field| {
                    let value = self.get(field.name).unwrap();
                    format!("{}: {:?}", field.name, value)
                })
                .collect::<Vec<_>>())
            .finish()
    }
}

impl<T: BitValue> fmt::Display for Bitfield<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Bitfield {{ ")?;
        let mut first = true;
        for field in self.fields.iter().filter(|f| matches!(f.access, Access::Read | Access::ReadWrite)) {
            if !first {
                write!(f, ", ")?;
            }
            first = false;
            let value = self.get(field.name).unwrap();
            write!(f, "{}: {}", field.name, value)?;
        }
        write!(f, " }}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_bitfield() {
        let mut flags = Bitfield::<u8>::builder(Endianness::Little, 0)
            .field("flag1", 0, 1, Access::ReadWrite)
            .field("flag2", 1, 1, Access::ReadWrite)
            .field("value", 2, 6, Access::ReadWrite)
            .build();
            
        assert_eq!(flags.get("flag1"), Some(0));
        assert_eq!(flags.get("flag2"), Some(0));
        assert_eq!(flags.get("value"), Some(0));
        
        flags.set("flag1", 1);
        assert_eq!(flags.get("flag1"), Some(1));
        
        flags.set("value", 42);
        assert_eq!(flags.get("value"), Some(42));
        
        println!("{:?}", flags);
        println!("{}", flags);
    }
    
    #[test]
    fn test_bitfield_access() {
        let mut flags = Bitfield::<u8>::builder(Endianness::Little, 0)
            .field("read_only", 0, 1, Access::Read)
            .field("write_only", 1, 1, Access::Write)
            .field("read_write", 2, 1, Access::ReadWrite)
            .build();
            
        assert_eq!(flags.get("read_only"), Some(0));
        assert_eq!(flags.get("write_only"), None);
        assert_eq!(flags.get("read_write"), Some(0));
        
        assert_eq!(flags.set("read_only", 1), None);
        assert_eq!(flags.set("write_only", 1), Some(()));
        assert_eq!(flags.set("read_write", 1), Some(()));
    }
    
    #[test]
    #[should_panic(expected = "Field flag2 overlaps with field flag1")]
    fn test_field_overlap() {
        Bitfield::<u8>::builder(Endianness::Little, 0)
            .field("flag1", 0, 2, Access::ReadWrite)
            .field("flag2", 1, 2, Access::ReadWrite)
            .build();
    }
    
    #[test]
    fn test_nested_bitfield() {
        let mut outer = Bitfield::<u16>::builder(Endianness::Little, 0)
            .field("inner", 0, 8, Access::ReadWrite)
            .field("other", 8, 8, Access::ReadWrite)
            .build();
            
        let mut inner = Bitfield::<u8>::builder(Endianness::Little, 0)
            .field("flag1", 0, 1, Access::ReadWrite)
            .field("flag2", 1, 1, Access::ReadWrite)
            .field("value", 2, 6, Access::ReadWrite)
            .build();
            
        inner.set("flag1", 1);
        inner.set("value", 42);
        
        outer.set_nested("inner", inner);
        
        let inner = outer.nested::<u8>("inner").unwrap();
        assert_eq!(inner.get("flag1"), Some(1));
        assert_eq!(inner.get("value"), Some(42));
    }
} 