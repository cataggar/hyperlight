// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The Hyperlight Authors.

use alloc::borrow::ToOwned;
use alloc::ffi::CString;
use alloc::vec::Vec;
use core::ffi::{CStr, c_char};
use core::mem::ManuallyDrop;

use hyperlight_common::flatbuffer_wrappers::function_types::{ReturnType, ReturnValue};

use super::{FfiByteChunks, FfiVec};

/// The value held by an [`FfiReturnValue`].
#[repr(C)]
#[derive(Copy, Clone)]
#[allow(non_camel_case_types, non_snake_case)]
pub union FfiReturnValueUnion {
    pub Int: i32,
    pub UInt: u32,
    pub Long: i64,
    pub ULong: u64,
    pub Float: f32,
    pub Double: f64,
    pub Bool: bool,
    pub String: *mut c_char,
    pub VecBytes: FfiVec,
    pub ByteChunks: FfiByteChunks,
}

/// An owned FFI return value.
#[repr(C)]
#[allow(non_camel_case_types)]
pub struct FfiReturnValue {
    pub tag: ReturnType,
    pub value: FfiReturnValueUnion,
}

impl FfiReturnValue {
    pub fn int(value: i32) -> Self {
        Self {
            tag: ReturnType::Int,
            value: FfiReturnValueUnion { Int: value },
        }
    }

    pub fn uint(value: u32) -> Self {
        Self {
            tag: ReturnType::UInt,
            value: FfiReturnValueUnion { UInt: value },
        }
    }

    pub fn long(value: i64) -> Self {
        Self {
            tag: ReturnType::Long,
            value: FfiReturnValueUnion { Long: value },
        }
    }

    pub fn ulong(value: u64) -> Self {
        Self {
            tag: ReturnType::ULong,
            value: FfiReturnValueUnion { ULong: value },
        }
    }

    pub fn float(value: f32) -> Self {
        Self {
            tag: ReturnType::Float,
            value: FfiReturnValueUnion { Float: value },
        }
    }

    pub fn double(value: f64) -> Self {
        Self {
            tag: ReturnType::Double,
            value: FfiReturnValueUnion { Double: value },
        }
    }

    pub fn boolean(value: bool) -> Self {
        Self {
            tag: ReturnType::Bool,
            value: FfiReturnValueUnion { Bool: value },
        }
    }

    pub fn void() -> Self {
        Self {
            tag: ReturnType::Void,
            value: FfiReturnValueUnion { Int: 0 },
        }
    }

    pub fn string(value: &CStr) -> Self {
        Self {
            tag: ReturnType::String,
            value: FfiReturnValueUnion {
                String: value.to_owned().into_raw(),
            },
        }
    }

    pub fn vec_bytes(value: Vec<u8>) -> Self {
        Self {
            tag: ReturnType::VecBytes,
            // SAFETY: `FfiReturnValue` reclaims the allocation when consumed or dropped.
            value: FfiReturnValueUnion {
                VecBytes: unsafe { FfiVec::from_vec(value) },
            },
        }
    }

    /// # Safety
    ///
    /// Every pointer in `value` must reference its declared number of bytes.
    pub unsafe fn byte_chunks(value: FfiByteChunks) -> Self {
        Self {
            tag: ReturnType::ByteChunks,
            value: FfiReturnValueUnion {
                // SAFETY: required by the caller.
                ByteChunks: unsafe { value.copy_owned() },
            },
        }
    }

    /// Consume this value and transfer its payload into a Rust return value.
    ///
    /// # Safety
    ///
    /// The tag and union value must be unchanged from a value created by this
    /// type's constructors.
    pub unsafe fn into_return_value(self) -> ReturnValue {
        let value = ManuallyDrop::new(self);
        // SAFETY: the contract requires the tag to identify the initialized union field.
        unsafe {
            match value.tag {
                ReturnType::Int => ReturnValue::Int(value.value.Int),
                ReturnType::UInt => ReturnValue::UInt(value.value.UInt),
                ReturnType::Long => ReturnValue::Long(value.value.Long),
                ReturnType::ULong => ReturnValue::ULong(value.value.ULong),
                ReturnType::Float => ReturnValue::Float(value.value.Float),
                ReturnType::Double => ReturnValue::Double(value.value.Double),
                ReturnType::Bool => ReturnValue::Bool(value.value.Bool),
                ReturnType::Void => ReturnValue::Void(()),
                ReturnType::String => {
                    let value = CString::from_raw(value.value.String);
                    ReturnValue::String(value.to_string_lossy().into_owned())
                }
                ReturnType::VecBytes => ReturnValue::VecBytes(value.value.VecBytes.into_vec()),
                ReturnType::ByteChunks => {
                    ReturnValue::ByteChunks(value.value.ByteChunks.into_owned_bytes())
                }
            }
        }
    }
}

impl TryFrom<ReturnValue> for FfiReturnValue {
    type Error = ();

    fn try_from(value: ReturnValue) -> Result<Self, Self::Error> {
        Ok(match value {
            ReturnValue::Int(value) => Self::int(value),
            ReturnValue::UInt(value) => Self::uint(value),
            ReturnValue::Long(value) => Self::long(value),
            ReturnValue::ULong(value) => Self::ulong(value),
            ReturnValue::Float(value) => Self::float(value),
            ReturnValue::Double(value) => Self::double(value),
            ReturnValue::Bool(value) => Self::boolean(value),
            ReturnValue::Void(()) => Self::void(),
            ReturnValue::String(value) => {
                let value = CString::new(value).map_err(|_| ())?;
                Self::string(&value)
            }
            ReturnValue::VecBytes(value) => Self::vec_bytes(value),
            ReturnValue::ByteChunks(value) => Self {
                tag: ReturnType::ByteChunks,
                value: FfiReturnValueUnion {
                    ByteChunks: FfiByteChunks::from_bytes(value),
                },
            },
        })
    }
}

impl Drop for FfiReturnValue {
    fn drop(&mut self) {
        // SAFETY: constructors initialize the owned field selected by the tag.
        unsafe {
            match self.tag {
                ReturnType::String => drop(CString::from_raw(self.value.String)),
                ReturnType::VecBytes => drop(self.value.VecBytes.into_vec()),
                ReturnType::ByteChunks => self.value.ByteChunks.drop_owned(),
                _ => {}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use hyperlight_common::flatbuffer_wrappers::function_types::Bytes;

    use super::*;

    #[test]
    fn return_values_round_trip_through_owned_ffi_values() {
        let values = [
            ReturnValue::Int(-1),
            ReturnValue::UInt(2),
            ReturnValue::Long(-3),
            ReturnValue::ULong(4),
            ReturnValue::Float(5.5),
            ReturnValue::Double(6.5),
            ReturnValue::Bool(true),
            ReturnValue::Void(()),
            ReturnValue::String("hello".to_string()),
            ReturnValue::VecBytes(vec![1, 2, 3]),
            ReturnValue::ByteChunks(vec![
                Bytes::copy_from_slice(b"hello"),
                Bytes::copy_from_slice(b"world"),
            ]),
        ];

        for expected in values {
            let ffi = FfiReturnValue::try_from(expected.clone()).unwrap();
            // SAFETY: `ffi` was created by `FfiReturnValue`.
            let actual = unsafe { ffi.into_return_value() };
            assert_eq!(actual, expected);
        }
    }

    #[test]
    fn string_with_nul_is_rejected() {
        assert!(FfiReturnValue::try_from(ReturnValue::String("a\0b".to_string())).is_err());
    }
}
