//! Typed accessors for Bismark-relevant SAM optional tags.
//!
//! Free functions over [`noodles_sam::alignment::record_buf::data::Data`]
//! that pull out tags by name and return typed slices/values. Missing
//! required tags surface as [`BismarkIoError::MissingTag`]; malformed
//! values surface as [`BismarkIoError::MalformedTag`].

use noodles_sam::alignment::record_buf::Data;
use noodles_sam::alignment::record_buf::data::field::Value;

use crate::io::error::BismarkIoError;

const TAG_XM: [u8; 2] = *b"XM";
const TAG_XR: [u8; 2] = *b"XR";
const TAG_XG: [u8; 2] = *b"XG";
const TAG_MD: [u8; 2] = *b"MD";
const TAG_NM: [u8; 2] = *b"NM";
const TAG_RX: [u8; 2] = *b"RX";
const TAG_MK: [u8; 2] = *b"mk";

/// Extract the `XM:Z:` methylation-call string from a record's data.
pub fn xm(data: &Data) -> Result<&[u8], BismarkIoError> {
    string_tag(data, &TAG_XM, "XM")
}

/// Extract the `XR:Z:` read-conversion tag.
pub fn xr(data: &Data) -> Result<&[u8], BismarkIoError> {
    string_tag(data, &TAG_XR, "XR")
}

/// Extract the `XG:Z:` genome-conversion tag.
pub fn xg(data: &Data) -> Result<&[u8], BismarkIoError> {
    string_tag(data, &TAG_XG, "XG")
}

/// Extract the optional `MD:Z:` mismatching-positions tag.
///
/// Returns `Ok(None)` if the tag is absent. Returns
/// [`BismarkIoError::MalformedTag`] if present but not a string.
pub fn md(data: &Data) -> Result<Option<&[u8]>, BismarkIoError> {
    optional_string_tag(data, &TAG_MD, "MD")
}

/// Extract the optional `NM:i:` edit-distance tag.
///
/// Returns `Ok(None)` if the tag is absent. Returns
/// [`BismarkIoError::MalformedTag`] if present but not a non-negative
/// integer that fits in u32.
pub fn nm(data: &Data) -> Result<Option<u32>, BismarkIoError> {
    let tag = noodles_sam::alignment::record::data::field::Tag::from(TAG_NM);
    match data.get(&tag) {
        None => Ok(None),
        Some(Value::Int8(v)) => to_u32_nonneg(*v as i64, "NM"),
        Some(Value::UInt8(v)) => Ok(Some(u32::from(*v))),
        Some(Value::Int16(v)) => to_u32_nonneg(*v as i64, "NM"),
        Some(Value::UInt16(v)) => Ok(Some(u32::from(*v))),
        Some(Value::Int32(v)) => to_u32_nonneg(*v as i64, "NM"),
        Some(Value::UInt32(v)) => Ok(Some(*v)),
        Some(other) => Err(BismarkIoError::MalformedTag {
            tag: "NM",
            reason: format!("expected integer, got {other:?}"),
        }),
    }
}

/// Extract the optional `RX:Z:` raw-UMI tag (SAM standard; written by the 5-Base path
/// when `--five_base_umi_len > 0`). Returns `Ok(None)` if absent.
pub fn rx(data: &Data) -> Result<Option<&[u8]>, BismarkIoError> {
    optional_string_tag(data, &TAG_RX, "RX")
}

/// Extract the optional 5-Base `mk:B:I` tag (#1211) as `(5' bases, 3' bases)` masked in read
/// orientation. Returns `Ok(None)` if absent.
pub fn mk(data: &Data) -> Result<Option<(usize, usize)>, BismarkIoError> {
    use noodles_sam::alignment::record_buf::data::field::value::Array;
    let tag = noodles_sam::alignment::record::data::field::Tag::from(TAG_MK);
    let value = match data.get(&tag) {
        None => return Ok(None),
        Some(v) => v,
    };
    let ends: Option<Vec<i64>> = match value {
        Value::Array(Array::Int8(v)) => Some(v.iter().map(|&x| i64::from(x)).collect()),
        Value::Array(Array::UInt8(v)) => Some(v.iter().map(|&x| i64::from(x)).collect()),
        Value::Array(Array::Int16(v)) => Some(v.iter().map(|&x| i64::from(x)).collect()),
        Value::Array(Array::UInt16(v)) => Some(v.iter().map(|&x| i64::from(x)).collect()),
        Value::Array(Array::Int32(v)) => Some(v.iter().map(|&x| i64::from(x)).collect()),
        Value::Array(Array::UInt32(v)) => Some(v.iter().map(|&x| i64::from(x)).collect()),
        _ => None,
    };
    match ends.as_deref() {
        Some(&[a, b]) if a >= 0 && b >= 0 => Ok(Some((a as usize, b as usize))),
        _ => Err(BismarkIoError::MalformedTag {
            tag: "mk",
            reason: format!("expected B with 2 non-negative integers, got {value:?}"),
        }),
    }
}

fn string_tag<'a>(
    data: &'a Data,
    tag_bytes: &[u8; 2],
    tag_name: &'static str,
) -> Result<&'a [u8], BismarkIoError> {
    let tag = noodles_sam::alignment::record::data::field::Tag::from(*tag_bytes);
    match data.get(&tag) {
        None => Err(BismarkIoError::MissingTag { tag: tag_name }),
        Some(Value::String(s)) => Ok(s.as_ref()),
        Some(other) => Err(BismarkIoError::MalformedTag {
            tag: tag_name,
            reason: format!("expected Z (string), got {other:?}"),
        }),
    }
}

fn optional_string_tag<'a>(
    data: &'a Data,
    tag_bytes: &[u8; 2],
    tag_name: &'static str,
) -> Result<Option<&'a [u8]>, BismarkIoError> {
    let tag = noodles_sam::alignment::record::data::field::Tag::from(*tag_bytes);
    match data.get(&tag) {
        None => Ok(None),
        Some(Value::String(s)) => Ok(Some(s.as_ref())),
        Some(other) => Err(BismarkIoError::MalformedTag {
            tag: tag_name,
            reason: format!("expected Z (string), got {other:?}"),
        }),
    }
}

fn to_u32_nonneg(v: i64, tag: &'static str) -> Result<Option<u32>, BismarkIoError> {
    if v < 0 {
        return Err(BismarkIoError::MalformedTag {
            tag,
            reason: format!("expected non-negative integer, got {v}"),
        });
    }
    u32::try_from(v)
        .map(Some)
        .map_err(|_| BismarkIoError::MalformedTag {
            tag,
            reason: format!("value {v} does not fit in u32"),
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use bstr::BString;
    use noodles_sam::alignment::record::data::field::Tag;

    fn make_data() -> Data {
        Data::default()
    }

    #[test]
    fn xm_missing_returns_missing_tag_error() {
        let data = make_data();
        let err = xm(&data).unwrap_err();
        assert!(matches!(err, BismarkIoError::MissingTag { tag: "XM" }));
    }

    #[test]
    fn xm_present_returns_bytes() {
        let mut data = make_data();
        data.insert(Tag::from(TAG_XM), Value::String(BString::from("z.h.x.")));
        let xm_bytes = xm(&data).unwrap();
        assert_eq!(xm_bytes, b"z.h.x.");
    }

    #[test]
    fn xr_xg_present_returns_bytes() {
        let mut data = make_data();
        data.insert(Tag::from(TAG_XR), Value::String(BString::from("CT")));
        data.insert(Tag::from(TAG_XG), Value::String(BString::from("GA")));
        assert_eq!(xr(&data).unwrap(), b"CT");
        assert_eq!(xg(&data).unwrap(), b"GA");
    }

    #[test]
    fn md_absent_returns_none() {
        let data = make_data();
        assert_eq!(md(&data).unwrap(), None);
    }

    #[test]
    fn md_present_returns_some_bytes() {
        let mut data = make_data();
        data.insert(Tag::from(TAG_MD), Value::String(BString::from("10A5")));
        assert_eq!(md(&data).unwrap(), Some(b"10A5".as_ref()));
    }

    #[test]
    fn nm_absent_returns_none() {
        let data = make_data();
        assert_eq!(nm(&data).unwrap(), None);
    }

    #[test]
    fn nm_present_as_int32_returns_value() {
        let mut data = make_data();
        data.insert(Tag::from(TAG_NM), Value::Int32(7));
        assert_eq!(nm(&data).unwrap(), Some(7));
    }

    #[test]
    fn nm_present_as_uint8_returns_value() {
        let mut data = make_data();
        data.insert(Tag::from(TAG_NM), Value::UInt8(3));
        assert_eq!(nm(&data).unwrap(), Some(3));
    }

    #[test]
    fn nm_negative_returns_malformed_tag_error() {
        let mut data = make_data();
        data.insert(Tag::from(TAG_NM), Value::Int32(-1));
        let err = nm(&data).unwrap_err();
        assert!(matches!(
            err,
            BismarkIoError::MalformedTag { tag: "NM", .. }
        ));
    }

    #[test]
    fn xr_wrong_type_returns_malformed_tag() {
        let mut data = make_data();
        data.insert(Tag::from(TAG_XR), Value::Int32(42));
        let err = xr(&data).unwrap_err();
        assert!(matches!(
            err,
            BismarkIoError::MalformedTag { tag: "XR", .. }
        ));
    }

    #[test]
    fn mk_reads_any_integer_array_subtype() {
        use noodles_sam::alignment::record_buf::data::field::value::Array;
        for value in [
            Array::UInt32(vec![3, 5]),
            Array::UInt8(vec![3, 5]),
            Array::Int16(vec![3, 5]),
            Array::Int32(vec![3, 5]),
        ] {
            let mut data = make_data();
            data.insert(Tag::from(TAG_MK), Value::Array(value));
            assert_eq!(mk(&data).unwrap(), Some((3, 5)));
        }
        assert_eq!(mk(&make_data()).unwrap(), None);
    }

    #[test]
    fn mk_rejects_wrong_length_negative_and_non_integer_values() {
        use noodles_sam::alignment::record_buf::data::field::value::Array;
        for value in [
            Value::Array(Array::UInt32(vec![3])),
            Value::Array(Array::Int32(vec![-1, 2])),
            Value::Array(Array::Float(vec![1.0, 2.0])),
            Value::String(BString::from("3,5")),
        ] {
            let mut data = make_data();
            data.insert(Tag::from(TAG_MK), value);
            assert!(matches!(
                mk(&data).unwrap_err(),
                BismarkIoError::MalformedTag { tag: "mk", .. }
            ));
        }
    }
}
