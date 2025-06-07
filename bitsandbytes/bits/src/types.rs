use std::fmt;
use std::ops::{BitAnd, BitOr, Not};
use std::num::NonZeroU8;
use std::convert::{Into, TryInto};

use crate::BitValue;

/// Error type for checked operations
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Value {0} exceeds maximum value for {1} bits")]
    ValueTooLarge(u128, usize),
    #[error("Value {0} is negative")]
    NegativeValue(i128),
}

/// Result type for checked operations
pub type Result<T> = std::result::Result<T, Error>;

/// Base trait for fixed-width integer types
pub trait FixedWidthInt: 
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
    
    /// Get the bit width of this type
    fn bit_width() -> usize;
    
    /// Get the maximum value for this type
    fn max_value() -> Self::IntType;
    
    /// Get the underlying value
    fn into_inner(self) -> Self::IntType;
    
    /// Create from the underlying value
    fn from_inner(value: Self::IntType) -> Self;
}

/// Trait for types that can be created with checked operations
pub trait Checked: FixedWidthInt {
    /// Create a new value, returning an error if the value is too large
    fn new(value: Self::IntType) -> Result<Self>;
    
    /// Create a new value, returning an error if the value is negative or too large
    fn new_signed(value: i128) -> Result<Self>;
}

/// Trait for types that can be created with unchecked operations
pub trait Unchecked: FixedWidthInt {
    /// Create a new value, truncating if necessary
    fn new_truncated(value: Self::IntType) -> Self;
    
    /// Create a new value, wrapping if necessary
    fn new_wrapping(value: Self::IntType) -> Self;
}

/// Trait for types that can be created with saturating operations
pub trait Saturating: FixedWidthInt {
    /// Create a new value, saturating at the maximum value if necessary
    fn new_saturating(value: Self::IntType) -> Self;
}

// Helper macro to implement common traits for fixed-width types
macro_rules! impl_fixed_width_int {
    ($t:ty, $width:expr, $storage:ty) => {
        impl FixedWidthInt for $t {
            type IntType = $storage;
            
            fn bit_width() -> usize {
                $width
            }
            
            fn max_value() -> Self::IntType {
                ((1u128 << $width) - 1) as $storage
            }
            
            fn into_inner(self) -> Self::IntType {
                self.0
            }
            
            fn from_inner(value: Self::IntType) -> Self {
                Self(value)
            }
        }
        
        impl BitAnd for $t {
            type Output = Self;
            
            fn bitand(self, rhs: Self) -> Self::Output {
                Self(self.0 & rhs.0)
            }
        }
        
        impl BitOr for $t {
            type Output = Self;
            
            fn bitor(self, rhs: Self) -> Self::Output {
                Self(self.0 | rhs.0)
            }
        }
        
        impl Not for $t {
            type Output = Self;
            
            fn not(self) -> Self::Output {
                Self(!self.0)
            }
        }
        
        impl fmt::Display for $t {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}", self.0)
            }
        }
        
        impl BitValue for $t {
            type IntType = $storage;
            
            fn from_int(value: Self::IntType) -> Self {
                Self::new_truncated(value)
            }
            
            fn to_int(self) -> Self::IntType {
                self.0
            }
            
            fn bit_width() -> usize {
                $width
            }
        }
    };
}

// Helper macro to implement checked operations
macro_rules! impl_checked {
    ($t:ty) => {
        impl Checked for $t {
            fn new(value: Self::IntType) -> Result<Self> {
                if value > Self::max_value() {
                    Err(Error::ValueTooLarge(value as u128, Self::bit_width()))
                } else {
                    Ok(Self(value))
                }
            }
            
            fn new_signed(value: i128) -> Result<Self> {
                if value < 0 {
                    Err(Error::NegativeValue(value))
                } else {
                    Self::new(value as Self::IntType)
                }
            }
        }
    };
}

// Helper macro to implement unchecked operations
macro_rules! impl_unchecked {
    ($t:ty) => {
        impl Unchecked for $t {
            fn new_truncated(value: Self::IntType) -> Self {
                Self(value & Self::max_value())
            }
            
            fn new_wrapping(value: Self::IntType) -> Self {
                Self(value % (Self::max_value() + 1))
            }
        }
    };
}

// Helper macro to implement saturating operations
macro_rules! impl_saturating {
    ($t:ty) => {
        impl Saturating for $t {
            fn new_saturating(value: Self::IntType) -> Self {
                if value > Self::max_value() {
                    Self(Self::max_value())
                } else {
                    Self(value)
                }
            }
        }
    };
}

// Helper macro to implement conversions
macro_rules! impl_conversions {
    ($t:ty, $storage:ty) => {
        // Implement Into for larger types
        impl Into<u8> for $t {
            fn into(self) -> u8 {
                self.0 as u8
            }
        }
        
        impl Into<u16> for $t {
            fn into(self) -> u16 {
                self.0 as u16
            }
        }
        
        impl Into<u32> for $t {
            fn into(self) -> u32 {
                self.0 as u32
            }
        }
        
        impl Into<u64> for $t {
            fn into(self) -> u64 {
                self.0 as u64
            }
        }
        
        impl Into<u128> for $t {
            fn into(self) -> u128 {
                self.0 as u128
            }
        }
        
        // Implement TryInto for smaller types
        impl TryInto<u8> for $t {
            type Error = Error;
            
            fn try_into(self) -> Result<u8> {
                if self.0 > u8::MAX as $storage {
                    Err(Error::ValueTooLarge(self.0 as u128, 8))
                } else {
                    Ok(self.0 as u8)
                }
            }
        }
        
        impl TryInto<u16> for $t {
            type Error = Error;
            
            fn try_into(self) -> Result<u16> {
                if self.0 > u16::MAX as $storage {
                    Err(Error::ValueTooLarge(self.0 as u128, 16))
                } else {
                    Ok(self.0 as u16)
                }
            }
        }
        
        impl TryInto<u32> for $t {
            type Error = Error;
            
            fn try_into(self) -> Result<u32> {
                if self.0 > u32::MAX as $storage {
                    Err(Error::ValueTooLarge(self.0 as u128, 32))
                } else {
                    Ok(self.0 as u32)
                }
            }
        }
        
        impl TryInto<u64> for $t {
            type Error = Error;
            
            fn try_into(self) -> Result<u64> {
                if self.0 > u64::MAX as $storage {
                    Err(Error::ValueTooLarge(self.0 as u128, 64))
                } else {
                    Ok(self.0 as u64)
                }
            }
        }
        
        // Implement From for smaller types
        impl From<u8> for $t {
            fn from(value: u8) -> Self {
                Self::new_truncated(value as $storage)
            }
        }
        
        impl From<u16> for $t {
            fn from(value: u16) -> Self {
                Self::new_truncated(value as $storage)
            }
        }
        
        impl From<u32> for $t {
            fn from(value: u32) -> Self {
                Self::new_truncated(value as $storage)
            }
        }
        
        impl From<u64> for $t {
            fn from(value: u64) -> Self {
                Self::new_truncated(value as $storage)
            }
        }
        
        impl From<u128> for $t {
            fn from(value: u128) -> Self {
                Self::new_truncated(value as $storage)
            }
        }
        
        // Implement TryFrom for larger types
        impl TryFrom<u8> for $t {
            type Error = Error;
            
            fn try_from(value: u8) -> Result<Self> {
                Self::new(value as $storage)
            }
        }
        
        impl TryFrom<u16> for $t {
            type Error = Error;
            
            fn try_from(value: u16) -> Result<Self> {
                Self::new(value as $storage)
            }
        }
        
        impl TryFrom<u32> for $t {
            type Error = Error;
            
            fn try_from(value: u32) -> Result<Self> {
                Self::new(value as $storage)
            }
        }
        
        impl TryFrom<u64> for $t {
            type Error = Error;
            
            fn try_from(value: u64) -> Result<Self> {
                Self::new(value as $storage)
            }
        }
        
        impl TryFrom<u128> for $t {
            type Error = Error;
            
            fn try_from(value: u128) -> Result<Self> {
                Self::new(value as $storage)
            }
        }

        // Implement From<usize> for ergonomic initialization
        impl From<usize> for $t {
            fn from(value: usize) -> Self {
                Self::new_truncated(value as $storage)
            }
        }

        // Implement TryFrom<usize> for checked initialization
        impl TryFrom<usize> for $t {
            type Error = Error;
            
            fn try_from(value: usize) -> Result<Self> {
                Self::new(value as $storage)
            }
        }
    };
}

// Helper macro to implement all operations for a type
macro_rules! impl_all {
    ($t:ty, $width:expr, $storage:ty) => {
        impl_fixed_width_int!($t, $width, $storage);
        impl_checked!($t);
        impl_unchecked!($t);
        impl_saturating!($t);
        impl_conversions!($t, $storage);
    };
}

// Define all fixed-width types
macro_rules! define_fixed_width_types {
    ($($t:ty, $width:expr, $storage:ty;)*) => {
        $(
            #[derive(Debug, Clone, Copy, PartialEq, Eq)]
            pub struct $t($storage);
            
            impl_all!($t, $width, $storage);
        )*
    };
}

// Define all the types
define_fixed_width_types! {
    u1, 1, u8;
    u2, 2, u8;
    u3, 3, u8;
    u4, 4, u8;
    u5, 5, u8;
    u6, 6, u8;
    u7, 7, u8;
    u9, 9, u16;
    u10, 10, u16;
    u11, 11, u16;
    u12, 12, u16;
    u13, 13, u16;
    u14, 14, u16;
    u15, 15, u16;
    u17, 17, u32;
    u18, 18, u32;
    u19, 19, u32;
    u20, 20, u32;
    u21, 21, u32;
    u22, 22, u32;
    u23, 23, u32;
    u24, 24, u32;
    u25, 25, u32;
    u26, 26, u32;
    u27, 27, u32;
    u28, 28, u32;
    u29, 29, u32;
    u30, 30, u32;
    u31, 31, u32;
    u33, 33, u64;
    u34, 34, u64;
    u35, 35, u64;
    u36, 36, u64;
    u37, 37, u64;
    u38, 38, u64;
    u39, 39, u64;
    u40, 40, u64;
    u41, 41, u64;
    u42, 42, u64;
    u43, 43, u64;
    u44, 44, u64;
    u45, 45, u64;
    u46, 46, u64;
    u47, 47, u64;
    u48, 48, u64;
    u49, 49, u64;
    u50, 50, u64;
    u51, 51, u64;
    u52, 52, u64;
    u53, 53, u64;
    u54, 54, u64;
    u55, 55, u64;
    u56, 56, u64;
    u57, 57, u64;
    u58, 58, u64;
    u59, 59, u64;
    u60, 60, u64;
    u61, 61, u64;
    u62, 62, u64;
    u63, 63, u64;
    u65, 65, u128;
    u66, 66, u128;
    u67, 67, u128;
    u68, 68, u128;
    u69, 69, u128;
    u70, 70, u128;
    u71, 71, u128;
    u72, 72, u128;
    u73, 73, u128;
    u74, 74, u128;
    u75, 75, u128;
    u76, 76, u128;
    u77, 77, u128;
    u78, 78, u128;
    u79, 79, u128;
    u80, 80, u128;
    u81, 81, u128;
    u82, 82, u128;
    u83, 83, u128;
    u84, 84, u128;
    u85, 85, u128;
    u86, 86, u128;
    u87, 87, u128;
    u88, 88, u128;
    u89, 89, u128;
    u90, 90, u128;
    u91, 91, u128;
    u92, 92, u128;
    u93, 93, u128;
    u94, 94, u128;
    u95, 95, u128;
    u96, 96, u128;
    u97, 97, u128;
    u98, 98, u128;
    u99, 99, u128;
    u100, 100, u128;
    u101, 101, u128;
    u102, 102, u128;
    u103, 103, u128;
    u104, 104, u128;
    u105, 105, u128;
    u106, 106, u128;
    u107, 107, u128;
    u108, 108, u128;
    u109, 109, u128;
    u110, 110, u128;
    u111, 111, u128;
    u112, 112, u128;
    u113, 113, u128;
    u114, 114, u128;
    u115, 115, u128;
    u116, 116, u128;
    u117, 117, u128;
    u118, 118, u128;
    u119, 119, u128;
    u120, 120, u128;
    u121, 121, u128;
    u122, 122, u128;
    u123, 123, u128;
    u124, 124, u128;
    u125, 125, u128;
    u126, 126, u128;
    u127, 127, u128;
}

// Create namespaces for different value handling strategies
pub mod checked {
    use super::*;
    
    /// Create a new value, returning an error if the value is too large
    pub fn u1(value: u8) -> Result<super::u1> { super::u1::new(value) }
    pub fn u2(value: u8) -> Result<super::u2> { super::u2::new(value) }
    pub fn u3(value: u8) -> Result<super::u3> { super::u3::new(value) }
    // ... implement for all types
}

pub mod unchecked {
    use super::*;
    
    /// Create a new value, truncating if necessary
    pub fn u1(value: u8) -> super::u1 { super::u1::new_truncated(value) }
    pub fn u2(value: u8) -> super::u2 { super::u2::new_truncated(value) }
    pub fn u3(value: u8) -> super::u3 { super::u3::new_truncated(value) }
    // ... implement for all types
}

pub mod wrapping {
    use super::*;
    
    /// Create a new value, wrapping if necessary
    pub fn u1(value: u8) -> super::u1 { super::u1::new_wrapping(value) }
    pub fn u2(value: u8) -> super::u2 { super::u2::new_wrapping(value) }
    pub fn u3(value: u8) -> super::u3 { super::u3::new_wrapping(value) }
    // ... implement for all types
}

pub mod saturating {
    use super::*;
    
    /// Create a new value, saturating at the maximum value if necessary
    pub fn u1(value: u8) -> super::u1 { super::u1::new_saturating(value) }
    pub fn u2(value: u8) -> super::u2 { super::u2::new_saturating(value) }
    pub fn u3(value: u8) -> super::u3 { super::u3::new_saturating(value) }
    // ... implement for all types
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_checked() {
        assert!(checked::u3(7).is_ok());
        assert!(checked::u3(8).is_err());
    }
    
    #[test]
    fn test_unchecked() {
        assert_eq!(unchecked::u3(7).into_inner(), 7);
        assert_eq!(unchecked::u3(8).into_inner(), 0);
    }
    
    #[test]
    fn test_wrapping() {
        assert_eq!(wrapping::u3(7).into_inner(), 7);
        assert_eq!(wrapping::u3(8).into_inner(), 0);
        assert_eq!(wrapping::u3(9).into_inner(), 1);
    }
    
    #[test]
    fn test_saturating() {
        assert_eq!(saturating::u3(7).into_inner(), 7);
        assert_eq!(saturating::u3(8).into_inner(), 7);
        assert_eq!(saturating::u3(9).into_inner(), 7);
    }
    
    #[test]
    fn test_conversions() {
        // Test Into
        let value = u7(100);
        let u8_value: u8 = value.into();
        assert_eq!(u8_value, 100);
        
        // Test TryInto
        let value = u7(100);
        let u8_value: Result<u8> = value.try_into();
        assert!(u8_value.is_ok());
        
        // Test From
        let value = u7::from(100u8);
        assert_eq!(value.into_inner(), 100);
        
        // Test TryFrom
        let value = u7::try_from(100u8);
        assert!(value.is_ok());
        
        // Test overflow
        let value = u7(200);
        let u8_value: Result<u8> = value.try_into();
        assert!(u8_value.is_err());
    }

    #[test]
    fn test_usize_conversions() {
        // Test From<usize>
        let value = u7::from(100usize);
        assert_eq!(value.into_inner(), 100);
        
        // Test TryFrom<usize>
        let value = u7::try_from(100usize);
        assert!(value.is_ok());
        
        // Test overflow
        let value = u7::try_from(200usize);
        assert!(value.is_err());
    }

    #[test]
    fn test_struct_initialization() {
        #[derive(Debug)]
        struct TestStruct {
            a: u3,
            b: u4,
            c: u1,
        }

        // Can now initialize directly with usize
        let test = TestStruct {
            a: 5.into(),  // or just 5
            b: 10.into(), // or just 10
            c: 1,  // or just 1
        };

        assert_eq!(test.a.into_inner(), 5);
        assert_eq!(test.b.into_inner(), 10);
        assert_eq!(test.c.into_inner(), 1);
    }
}