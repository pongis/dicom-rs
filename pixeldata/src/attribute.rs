//! Utility module for fetching key attributes from a DICOM object.

use dicom_core::{header::HasLength, DataDictionary, Tag};
use dicom_dictionary_std::tags;
use dicom_object::{mem::InMemElement, FileDicomObject, InMemDicomObject};
use snafu::{ensure, Backtrace, OptionExt, ResultExt, Snafu};
use std::fmt;

/// An enum for a DICOM attribute which can be retrieved
/// for the purposes of decoding pixel data.
///
/// Since the set of attributes needed is more constrained,
/// this is a more compact representation than a tag or a static string.
#[derive(Debug, Copy, Clone)]
#[non_exhaustive]
pub enum AttributeName {
    Columns,
    Rows,
    BitsAllocated,
    BitsStored,
    HighBit,
    NumberOfFrames,
    PhotometricInterpretation,
    PixelData,
    PixelRepresentation,
    PlanarConfiguration,
    RescaleSlope,
    RescaleIntercept,
    SamplesPerPixel,
    VoiLutFunction,
    WindowCenter,
    WindowWidth,
    LutDescriptor,
    LutData,
    LutExplanation,
}

impl std::fmt::Display for AttributeName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AttributeName::VoiLutFunction => f.write_str("VOILUTFunction"),
            _ => std::fmt::Debug::fmt(self, f),
        }
    }
}

#[derive(Debug, Snafu)]
pub enum GetAttributeError {
    #[snafu(display("Missing required attribute `{}`", name))]
    MissingRequired {
        name: AttributeName,
        backtrace: Backtrace,
    },

    #[snafu(display("Could not retrieve attribute `{}`", name))]
    Retrieve {
        name: AttributeName,
        #[snafu(backtrace)]
        #[snafu(source(from(dicom_object::AccessError, Box::from)))]
        source: Box<dicom_object::AccessError>,
    },

    #[snafu(display("Could not get attribute `{}`", name))]
    CastValue {
        name: AttributeName,
        source: dicom_core::value::CastValueError,
        backtrace: Backtrace,
    },

    #[snafu(display("Could not convert attribute `{}`", name))]
    ConvertValue {
        name: AttributeName,
        #[snafu(source(from(dicom_core::value::ConvertValueError, Box::from)))]
        source: Box<dicom_core::value::ConvertValueError>,
        backtrace: Backtrace,
    },

    #[snafu(display("Semantically invalid value `{}` for attribute `{}`", value, name))]
    InvalidValue {
        name: AttributeName,
        value: String,
        backtrace: Backtrace,
    },
}

pub type Result<T, E = GetAttributeError> = std::result::Result<T, E>;

/// Get the Columns from the DICOM object
pub fn cols<D: DataDictionary + Clone>(obj: &FileDicomObject<InMemDicomObject<D>>) -> Result<u16> {
    retrieve_required_u16(obj, tags::COLUMNS, AttributeName::Columns)
}

/// Get the Rows from the DICOM object
pub fn rows<D: DataDictionary + Clone>(obj: &FileDicomObject<InMemDicomObject<D>>) -> Result<u16> {
    retrieve_required_u16(obj, tags::ROWS, AttributeName::Rows)
}

/// Get the VOILUTFunction from the DICOM object
pub fn voi_lut_function<D: DataDictionary + Clone>(
    obj: &FileDicomObject<InMemDicomObject<D>>,
) -> Result<Option<Vec<String>>> {
    let elems = obj
        .element(tags::VOILUT_FUNCTION)
        .ok()
        .map(|v| vec![v])
        .or_else(|| {
            get_from_shared(obj, [tags::FRAME_VOILUT_SEQUENCE, tags::VOILUT_FUNCTION])
                .map(|inner| inner.collect())
        })
        .or_else(|| {
            get_from_per_frame(obj, [tags::FRAME_VOILUT_SEQUENCE, tags::VOILUT_FUNCTION])
                .map(|inner| inner.collect())
        });
    if let Some(elems_inner) = elems {
        let res = elems_inner
            .iter()
            .map(|el| {
                (*el)
                    .string()
                    .context(CastValueSnafu {
                        name: AttributeName::VoiLutFunction,
                    })
                    .map(|v| v.trim().to_string())
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(Some(res))
    } else {
        Ok(Some(vec!["LINEAR".to_string()]))
    }
}

/// Get the SamplesPerPixel from the DICOM object
pub fn samples_per_pixel<D: DataDictionary + Clone>(
    obj: &FileDicomObject<InMemDicomObject<D>>,
) -> Result<u16> {
    retrieve_required_u16(obj, tags::SAMPLES_PER_PIXEL, AttributeName::SamplesPerPixel)
}

/// Get the BitsAllocated from the DICOM object
pub fn bits_allocated<D: DataDictionary + Clone>(
    obj: &FileDicomObject<InMemDicomObject<D>>,
) -> Result<u16> {
    retrieve_required_u16(obj, tags::BITS_ALLOCATED, AttributeName::BitsAllocated)
}

/// Get the BitsStored from the DICOM object
pub fn bits_stored<D: DataDictionary + Clone>(
    obj: &FileDicomObject<InMemDicomObject<D>>,
) -> Result<u16> {
    retrieve_required_u16(obj, tags::BITS_STORED, AttributeName::BitsStored)
}

/// Get the HighBit from the DICOM object
pub fn high_bit<D: DataDictionary + Clone>(
    obj: &FileDicomObject<InMemDicomObject<D>>,
) -> Result<u16> {
    retrieve_required_u16(obj, tags::HIGH_BIT, AttributeName::HighBit)
}

/// Get the PixelData element from the DICOM object
pub fn pixel_data<D: DataDictionary + Clone>(
    obj: &FileDicomObject<InMemDicomObject<D>>,
) -> Result<&InMemElement<D>> {
    let name = AttributeName::PixelData;
    obj.element_opt(tags::PIXEL_DATA)
        .context(RetrieveSnafu { name })?
        .context(MissingRequiredSnafu { name })
}

fn get_from_shared<D: DataDictionary + Clone>(
    obj: &FileDicomObject<InMemDicomObject<D>>,
    selector: [Tag; 2],
) -> Option<impl Iterator<Item = &InMemElement<D>>> {
    let items = obj
        .get(tags::SHARED_FUNCTIONAL_GROUPS_SEQUENCE)?
        .items()?
        .first()?
        .get(selector[0])
        .and_then(|inner| inner.items()?.first()?.get(selector[1]))
        // Sometimes the tag is not in the properly nested sequence, but just flat in the first
        // element of the SharedFunctionalGroupsSequence
        .or_else(|| {
            obj.get(tags::SHARED_FUNCTIONAL_GROUPS_SEQUENCE)?
                .items()?
                .first()?
                .get(selector[1])
        })
        .map(std::iter::once);

    items.and_then(|items| {
        let mut peekable = items.peekable();
        peekable.peek().is_some().then_some(peekable)
    })
}

fn get_from_per_frame<D: DataDictionary + Clone>(
    obj: &FileDicomObject<InMemDicomObject<D>>,
    selector: [Tag; 2],
) -> Option<impl Iterator<Item = &InMemElement<D>>> {
    let mut items = obj
        .get(tags::PER_FRAME_FUNCTIONAL_GROUPS_SEQUENCE)?
        .items()?
        .iter()
        .filter_map(move |item| item.get(selector[0])?.items()?.first()?.get(selector[1]))
        .peekable();

    items.peek().is_some().then_some(items)
}

/// Get the RescaleIntercept from the DICOM object or returns 0
pub fn rescale_intercept<D: DataDictionary + Clone>(
    obj: &FileDicomObject<InMemDicomObject<D>>,
) -> Vec<f64> {
    obj.element(tags::RESCALE_INTERCEPT)
        .ok()
        .and_then(|e| {
            vec![e.to_float64().ok()]
                .into_iter()
                .collect::<Option<Vec<f64>>>()
        })
        .or_else(|| {
            get_from_per_frame(
                obj,
                [
                    tags::PIXEL_VALUE_TRANSFORMATION_SEQUENCE,
                    tags::RESCALE_INTERCEPT,
                ],
            )
            .and_then(|v| v.into_iter().map(|el| el.to_float64().ok()).collect())
        })
        .or_else(|| {
            get_from_shared(
                obj,
                [
                    tags::PIXEL_VALUE_TRANSFORMATION_SEQUENCE,
                    tags::RESCALE_INTERCEPT,
                ],
            )
            .and_then(|v| v.into_iter().map(|el| el.to_float64().ok()).collect())
        })
        .unwrap_or(vec![0.])
}

/// Get the RescaleSlope from the DICOM object or returns 1.0
pub fn rescale_slope<D: DataDictionary + Clone>(
    obj: &FileDicomObject<InMemDicomObject<D>>,
) -> Vec<f64> {
    obj.element(tags::RESCALE_SLOPE)
        .ok()
        .and_then(|e| {
            vec![e.to_float64().ok()]
                .into_iter()
                .collect::<Option<Vec<f64>>>()
        })
        .or_else(|| {
            get_from_per_frame(
                obj,
                [
                    tags::PIXEL_VALUE_TRANSFORMATION_SEQUENCE,
                    tags::RESCALE_SLOPE,
                ],
            )
            .and_then(|v| v.into_iter().map(|el| el.to_float64().ok()).collect())
        })
        .or_else(|| {
            get_from_shared(
                obj,
                [
                    tags::PIXEL_VALUE_TRANSFORMATION_SEQUENCE,
                    tags::RESCALE_SLOPE,
                ],
            )
            .and_then(|v| v.into_iter().map(|el| el.to_float64().ok()).collect())
        })
        .unwrap_or(vec![1.0])
}

/// Get the NumberOfFrames from the DICOM object,
/// returning 1 if it is not present
pub fn number_of_frames<D: DataDictionary + Clone>(
    obj: &FileDicomObject<InMemDicomObject<D>>,
) -> Result<u32> {
    let name = AttributeName::NumberOfFrames;
    let elem = if let Some(elem) = obj
        .element_opt(tags::NUMBER_OF_FRAMES)
        .context(RetrieveSnafu { name })?
    {
        elem
    } else {
        return Ok(1);
    };

    if elem.is_empty() {
        return Ok(1);
    }

    let integer = elem.to_int::<i32>().context(ConvertValueSnafu { name })?;

    ensure!(
        integer > 0,
        InvalidValueSnafu {
            name,
            value: integer.to_string(),
        }
    );

    Ok(integer as u32)
}

/// Retrieve the WindowCenter from the DICOM object if it exists.
pub fn window_center<D: DataDictionary + Clone>(
    obj: &FileDicomObject<InMemDicomObject<D>>,
) -> Option<Vec<f64>> {
    let wc = obj
        .get(tags::WINDOW_CENTER)
        .and_then(|e| {
            vec![e.to_float64().ok()]
                .into_iter()
                .collect::<Option<Vec<f64>>>()
        })
        .or_else(|| {
            get_from_per_frame(obj, [tags::FRAME_VOILUT_SEQUENCE, tags::WINDOW_CENTER])
                .and_then(|v| v.into_iter().map(|el| el.to_float64().ok()).collect())
        })
        .or_else(|| {
            get_from_shared(obj, [tags::FRAME_VOILUT_SEQUENCE, tags::WINDOW_CENTER])
                .and_then(|v| v.into_iter().map(|el| el.to_float64().ok()).collect())
        });
    wc
}

/// Retrieve the WindowWidth from the DICOM object if it exists.
pub fn window_width<D: DataDictionary + Clone>(
    obj: &FileDicomObject<InMemDicomObject<D>>,
) -> Option<Vec<f64>> {
    let ww = obj
        .get(tags::WINDOW_WIDTH)
        .and_then(|e| {
            vec![e.to_float64().ok()]
                .into_iter()
                .collect::<Option<Vec<f64>>>()
        })
        .or_else(|| {
            get_from_per_frame(obj, [tags::FRAME_VOILUT_SEQUENCE, tags::WINDOW_WIDTH])
                .and_then(|v| v.into_iter().map(|el| el.to_float64().ok()).collect())
        })
        .or_else(|| {
            get_from_shared(obj, [tags::FRAME_VOILUT_SEQUENCE, tags::WINDOW_WIDTH])
                .and_then(|v| v.into_iter().map(|el| el.to_float64().ok()).collect())
        });
    ww
}

#[inline]
fn retrieve_required_u16<D>(
    obj: &FileDicomObject<InMemDicomObject<D>>,
    tag: Tag,
    name: AttributeName,
) -> Result<u16>
where
    D: DataDictionary + Clone,
{
    obj.element_opt(tag)
        .context(RetrieveSnafu { name })?
        .context(MissingRequiredSnafu { name })?
        .uint16()
        .context(CastValueSnafu { name })
}

/// A decoded representation of the DICOM _Pixel Representation_ attribute.
#[derive(Debug, Copy, Clone, Eq, Hash, PartialEq)]
#[repr(u16)]
pub enum PixelRepresentation {
    /// 0: unsigned pixel data sample values
    Unsigned = 0,
    /// 1: signed pixel data sample values
    Signed = 1,
}

/// Get the PixelRepresentation from the DICOM object
pub fn pixel_representation<D: DataDictionary + Clone>(
    obj: &FileDicomObject<InMemDicomObject<D>>,
) -> Result<PixelRepresentation> {
    let p = retrieve_required_u16(
        obj,
        tags::PIXEL_REPRESENTATION,
        AttributeName::PixelRepresentation,
    )?;

    match p {
        0 => Ok(PixelRepresentation::Unsigned),
        1 => Ok(PixelRepresentation::Signed),
        _ => InvalidValueSnafu {
            name: AttributeName::PixelRepresentation,
            value: p.to_string(),
        }
        .fail(),
    }
}

/// A decoded representation of the DICOM _Planar Configuration_ attribute.
#[derive(Debug, Copy, Clone, Eq, Hash, PartialEq)]
#[repr(u16)]
pub enum PlanarConfiguration {
    /// 0: Standard planar configuration.
    /// Each pixel is encoded contiguously.
    Standard = 0,
    /// 1: Pixel-first planar configuration.
    /// Each color plane is encoded contiguously.
    PixelFirst = 1,
}

impl fmt::Display for PlanarConfiguration {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        (*self as u16).fmt(f)
    }
}

/// Get the PlanarConfiguration from the DICOM object,
/// returning the standard planar configuration by default
#[cfg(not(feature = "gdcm"))]
pub fn planar_configuration<D: DataDictionary + Clone>(
    obj: &FileDicomObject<InMemDicomObject<D>>,
) -> Result<PlanarConfiguration> {
    let elem = if let Some(elem) =
        obj.element_opt(tags::PLANAR_CONFIGURATION)
            .context(RetrieveSnafu {
                name: AttributeName::PlanarConfiguration,
            })? {
        elem
    } else {
        return Ok(PlanarConfiguration::Standard);
    };

    let p = elem.to_int::<i16>().context(ConvertValueSnafu {
        name: AttributeName::PlanarConfiguration,
    })?;

    match p {
        0 => Ok(PlanarConfiguration::Standard),
        1 => Ok(PlanarConfiguration::PixelFirst),
        _ => InvalidValueSnafu {
            name: AttributeName::PlanarConfiguration,
            value: p.to_string(),
        }
        .fail(),
    }
}

/// A decoded representation of the
/// DICOM _Photometric Interpretation_ attribute.
///
/// See [section C.7.6.3][1] of the standard
/// for more details about each photometric interpretation.
///
/// In the event that the photometric interpretation is not
/// any of the specified variants,
/// the `Other` variant is used.
/// Note that this enumeration covers the ones which are known,
/// not necessarily supported in the decoding process.
///
/// [1]: https://dicom.nema.org/medical/dicom/current/output/chtml/part03/sect_C.7.6.3.html#sect_C.7.6.3.1.2
#[derive(Debug, Clone, PartialEq)]
pub enum PhotometricInterpretation {
    /// `MONOCHROME1`:
    /// Pixel data represent a single monochrome image plane.
    /// The minimum sample value is intended to be displayed as white.
    Monochrome1,
    /// `MONOCHROME2`:
    /// Pixel data represent a single monochrome image plane.
    /// The minimum sample value is intended to be displayed as black.
    Monochrome2,
    /// `PALETTE COLOR`:
    /// Pixel data describe a color image with a single sample per pixel
    /// (single image plane).
    PaletteColor,
    /// `RGB`:
    /// Pixel data represent a color image described by
    /// red, green, and blue image planes.
    Rgb,
    /// `YBR_FULL`:
    /// Pixel data represent a color image described by
    /// one luminance (Y) and two chrominance planes (CB and CR)
    /// and as a result there are half as many CB and CR values as Y values.
    YbrFull,
    /// `YBR_FULL_422`:
    /// The same as YBR_FULL except that the CB and CR values
    /// are sampled horizontally at half the Y rate.
    YbrFull422,
    /// `YBR_PARTIAL_420`:
    /// Pixel data represent a color image described by
    /// one luminance (Y) and two chrominance planes (CB and CR).
    /// The CB and CR values are sampled
    /// horizontally and vertically at half the Y rate
    /// and as a result there are four times less CB and CR values than Y values.
    YbrPartial420,
    /// `YBR_ICT`:
    /// Irreversible Color Transformation.
    /// Pixel data represent a color image described by
    /// one luminance (Y) and two chrominance planes (CB and CR).
    YbrIct,
    /// `YBR_RCT`:
    /// Rreversible Color Transformation.
    /// Pixel data represent a color image described by
    /// one luminance (Y) and two chrominance planes (CB and CR).
    YbrRct,
    /// The photometric interpretation is not one of the known variants.
    ///
    /// **Note:** this value is assumed to be different from
    /// any other variant listed above,
    /// and no checks are made to ensure this assumption.
    /// The construction of `PhotometricInterpretation::Other`
    /// when one of the existing variants is applicable
    /// is considered a bug.
    ///
    /// **Note 2:** subsequent crate versions may introduce new variants,
    /// and as a consequence break any user depending on the `Other` variant.
    /// If you need to depend on an unspecified variant,
    /// you should also double check the photometric interpretations here
    /// every time the crate is updated.
    Other(String),
}

impl PhotometricInterpretation {
    /// Obtain a string representation of the photometric interpretation.
    pub fn as_str(&self) -> &str {
        self.as_ref()
    }

    /// Get whether this photometric interpretation is
    /// one of the monochrome variants
    /// (`MONOCHROME1` or `MONOCHROME2`).
    pub fn is_monochrome(&self) -> bool {
        matches!(
            self,
            PhotometricInterpretation::Monochrome1 | PhotometricInterpretation::Monochrome2
        )
    }
}

impl AsRef<str> for PhotometricInterpretation {
    fn as_ref(&self) -> &str {
        match self {
            PhotometricInterpretation::Monochrome1 => "MONOCHROME1",
            PhotometricInterpretation::Monochrome2 => "MONOCHROME2",
            PhotometricInterpretation::PaletteColor => "PALETTE COLOR",
            PhotometricInterpretation::Rgb => "RGB",
            PhotometricInterpretation::YbrFull => "YBR_FULL",
            PhotometricInterpretation::YbrFull422 => "YBR_FULL_422",
            PhotometricInterpretation::YbrPartial420 => "YBR_PARTIAL_420",
            PhotometricInterpretation::YbrIct => "YBR_ICT",
            PhotometricInterpretation::YbrRct => "YBR_RCT",
            PhotometricInterpretation::Other(s) => s,
        }
    }
}

impl From<String> for PhotometricInterpretation {
    fn from(s: String) -> Self {
        match s.as_str() {
            "MONOCHROME1" => PhotometricInterpretation::Monochrome1,
            "MONOCHROME2" => PhotometricInterpretation::Monochrome2,
            "PALETTE COLOR" => PhotometricInterpretation::PaletteColor,
            "RGB" => PhotometricInterpretation::Rgb,
            "YBR_FULL" => PhotometricInterpretation::YbrFull,
            "YBR_FULL_422" => PhotometricInterpretation::YbrFull422,
            "YBR_PARTIAL_420" => PhotometricInterpretation::YbrPartial420,
            "YBR_ICT" => PhotometricInterpretation::YbrIct,
            "YBR_RCT" => PhotometricInterpretation::YbrRct,
            _ => PhotometricInterpretation::Other(s),
        }
    }
}

impl From<&str> for PhotometricInterpretation {
    fn from(s: &str) -> Self {
        match s {
            "MONOCHROME1" => PhotometricInterpretation::Monochrome1,
            "MONOCHROME2" => PhotometricInterpretation::Monochrome2,
            "PALETTE COLOR" => PhotometricInterpretation::PaletteColor,
            "RGB" => PhotometricInterpretation::Rgb,
            "YBR_FULL" => PhotometricInterpretation::YbrFull,
            "YBR_FULL_422" => PhotometricInterpretation::YbrFull422,
            "YBR_PARTIAL_420" => PhotometricInterpretation::YbrPartial420,
            "YBR_ICT" => PhotometricInterpretation::YbrIct,
            "YBR_RCT" => PhotometricInterpretation::YbrRct,
            _ => PhotometricInterpretation::Other(s.to_string()),
        }
    }
}

impl fmt::Display for PhotometricInterpretation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PhotometricInterpretation::Monochrome1 => f.write_str("MONOCHROME1"),
            PhotometricInterpretation::Monochrome2 => f.write_str("MONOCHROME2"),
            PhotometricInterpretation::PaletteColor => f.write_str("PALETTE COLOR"),
            PhotometricInterpretation::Rgb => f.write_str("RGB"),
            PhotometricInterpretation::YbrFull => f.write_str("YBR_FULL"),
            PhotometricInterpretation::YbrFull422 => f.write_str("YBR_FULL_422"),
            PhotometricInterpretation::YbrPartial420 => f.write_str("YBR_PARTIAL_420"),
            PhotometricInterpretation::YbrIct => f.write_str("YBR_ICT"),
            PhotometricInterpretation::YbrRct => f.write_str("YBR_RCT"),
            PhotometricInterpretation::Other(s) => f.write_str(s),
        }
    }
}

/// Get the PhotoMetricInterpretation from the DICOM object
pub fn photometric_interpretation<D: DataDictionary + Clone>(
    obj: &FileDicomObject<InMemDicomObject<D>>,
) -> Result<PhotometricInterpretation> {
    let name = AttributeName::PhotometricInterpretation;
    Ok(obj
        .element_opt(tags::PHOTOMETRIC_INTERPRETATION)
        .context(RetrieveSnafu { name })?
        .context(MissingRequiredSnafu { name })?
        .string()
        .context(CastValueSnafu { name })?
        .trim_matches(|c: char| c.is_whitespace() || c == '\0')
        .into())
}

/// A decoded representation of the
/// DICOM _VOI LUT Sequence_ attribute.
///
/// See [section C.8.11.3.1.5][1] of the standard for more details.
///
/// [1]: https://dicom.nema.org/medical/dicom/2024d/output/chtml/part03/sect_C.8.11.3.html#sect_C.8.11.3.1.5
#[derive(Clone, Debug)]
pub struct VoiLut {
    /// Minimum pixel value to be mapped. All values below this should be mapped
    /// to the first entry of the table.
    ///
    /// The value can either be a signed or an unsigned 16-bit integer, an i32
    /// accomodates both.
    pub min_pixel_value: i32,
    /// Number of bits stored for each LUT entry
    pub bits_stored: u8,
    /// LUT data. Pixels with value min_pixel_value or below get mapped to the
    /// first entry, pixels with value min_pixel_value+1 to the second entry,
    /// and so forth. Pixels with value higher or equal to min_pixel_value +
    /// data.len() get mapped to the last entry.
    pub data: Vec<u16>,
    ///  Free form text explanation of the meaning of the LUT.
    pub explanation: Option<String>,
}

fn parse_voi_lut_entry<D: DataDictionary + Clone>(entry: &InMemDicomObject<D>) -> Result<VoiLut> {
    let descriptor_elements: Vec<i32> = entry
        .element_opt(tags::LUT_DESCRIPTOR)
        .context(RetrieveSnafu {
            name: AttributeName::LutDescriptor,
        })?
        .context(MissingRequiredSnafu {
            name: AttributeName::LutDescriptor,
        })?
        .to_multi_int()
        .context(ConvertValueSnafu {
            name: AttributeName::LutDescriptor,
        })?;
    ensure!(
        descriptor_elements.len() == 3,
        InvalidValueSnafu {
            name: AttributeName::LutDescriptor,
            value: format!("value with multiplicity {}", descriptor_elements.len()),
        }
    );
    ensure!(
        descriptor_elements[0] > 0,
        InvalidValueSnafu {
            name: AttributeName::LutDescriptor,
            value: format!("value with LUT length {}", descriptor_elements[0]),
        }
    );
    let expected_lut_len: usize = descriptor_elements[0] as usize;

    let min_pixel_value = descriptor_elements[1];

    ensure!(
        descriptor_elements[2] >= 0 && descriptor_elements[2] <= 16,
        InvalidValueSnafu {
            name: AttributeName::LutDescriptor,
            value: format!("value with bits stored {}", descriptor_elements[2])
        }
    );
    let bits_stored = descriptor_elements[2] as u8;

    let lut_data = entry
        .element_opt(tags::LUT_DATA)
        .context(RetrieveSnafu {
            name: AttributeName::LutData,
        })?
        .context(MissingRequiredSnafu {
            name: AttributeName::LutData,
        })?
        .uint16_slice()
        .context(CastValueSnafu {
            name: AttributeName::LutData,
        })?;
    ensure!(
        expected_lut_len == lut_data.len(),
        InvalidValueSnafu {
            name: AttributeName::LutData,
            value: format!(
                "sequence with {} elements (expected {expected_lut_len})",
                lut_data.len()
            ),
        }
    );

    let explanation = if let Some(val) =
        entry
            .element_opt(tags::LUT_EXPLANATION)
            .context(RetrieveSnafu {
                name: AttributeName::LutExplanation,
            })? {
        Some(
            val.string()
                .context(CastValueSnafu {
                    name: AttributeName::LutExplanation,
                })?
                .to_string(),
        )
    } else {
        None
    };

    Ok(VoiLut {
        min_pixel_value,
        bits_stored,
        data: lut_data.to_vec(),
        explanation,
    })
}

/// Get the VOI LUT Sequence from the DICOM object
///
/// This function will look for the _VOI LUT Sequence_ (0028,3010) attribute
/// at the root, then the per frame functional groups,
/// and then the shared functional group.
pub fn voi_lut_sequence<D: DataDictionary + Clone>(
    obj: &FileDicomObject<InMemDicomObject<D>>,
) -> Option<Vec<VoiLut>> {
    obj.get(tags::VOILUT_SEQUENCE)
        .and_then(|e| {
            e.items().and_then(|items| {
                items
                    .iter()
                    .map(|e| parse_voi_lut_entry(e).ok())
                    .collect::<Option<Vec<VoiLut>>>()
            })
        })
        .or_else(|| {
            get_from_per_frame(obj, [tags::FRAME_VOILUT_SEQUENCE, tags::VOILUT_SEQUENCE]).and_then(
                |v| {
                    v.into_iter()
                        .flat_map(|el| el.items())
                        .flat_map(|items| items.iter().map(|e| parse_voi_lut_entry(e).ok()))
                        .collect()
                },
            )
        })
        .or_else(|| {
            get_from_shared(obj, [tags::FRAME_VOILUT_SEQUENCE, tags::VOILUT_SEQUENCE]).and_then(
                |v| {
                    v.into_iter()
                        .flat_map(|el| el.items())
                        .flat_map(|items| items.iter().map(|e| parse_voi_lut_entry(e).ok()))
                        .collect()
                },
            )
        })
}

#[cfg(test)]
mod tests {
    use super::rescale_intercept;
    use dicom_core::{
        dicom_value,
        ops::{ApplyOp, AttributeAction, AttributeOp},
        value::DataSetSequence,
        DataElement, PrimitiveValue, VR,
    };
    use dicom_dictionary_std::{tags, uids};
    use dicom_object::{
        DefaultDicomObject, FileDicomObject, FileMetaTableBuilder, InMemDicomObject,
    };

    #[test]
    fn errors_are_not_too_large() {
        let max_size = 88;
        let size = std::mem::size_of::<super::GetAttributeError>();
        assert!(
            size <= max_size,
            "GetAttributeError size is too large ({} > {})",
            size, max_size
        );
    }

    fn dummy_dicom() -> DefaultDicomObject {
        FileDicomObject::new_empty_with_meta(
            FileMetaTableBuilder::new()
                .transfer_syntax(uids::EXPLICIT_VR_LITTLE_ENDIAN)
                .media_storage_sop_class_uid(uids::ENHANCED_MR_IMAGE_STORAGE)
                .media_storage_sop_instance_uid("2.25.145929179730251416957282651365760465911")
                .build()
                .unwrap(),
        )
    }

    #[test]
    fn get_required_field_from_top_level_dataset() {
        let mut dcm = dummy_dicom();
        // Returns vec![0.] if not present
        assert_eq!(rescale_intercept(&dcm), vec![0.]);

        // Finds the correct value from top level dataset
        dcm.put_element(DataElement::new(
            tags::RESCALE_INTERCEPT,
            VR::DS,
            dicom_value!(F64, 1.0),
        ));
        assert_eq!(rescale_intercept(&dcm), vec![1.0]);
    }

    #[test]
    fn get_required_field_from_shared_fn_groups() {
        let mut dcm = dummy_dicom();
        // Add shared functional groups sequence
        dcm.apply(AttributeOp::new(
            tags::SHARED_FUNCTIONAL_GROUPS_SEQUENCE,
            AttributeAction::SetIfMissing(PrimitiveValue::Empty),
        ))
        .unwrap();
        // Check the fn still returns nothing.
        assert_eq!(rescale_intercept(&dcm), vec![0.0]);

        // Add the PixelValueTransformationSequence entry
        dcm.apply(AttributeOp::new(
            (
                tags::SHARED_FUNCTIONAL_GROUPS_SEQUENCE,
                0,
                tags::PIXEL_VALUE_TRANSFORMATION_SEQUENCE,
            ),
            AttributeAction::Set(PrimitiveValue::Empty),
        ))
        .unwrap();

        // Check the fn still returns nothing.
        assert_eq!(rescale_intercept(&dcm), vec![0.0]);
        dcm.apply(AttributeOp::new(
            (
                tags::SHARED_FUNCTIONAL_GROUPS_SEQUENCE,
                0,
                tags::PIXEL_VALUE_TRANSFORMATION_SEQUENCE,
                0,
                tags::RESCALE_INTERCEPT,
            ),
            AttributeAction::Set(dicom_value!(F64, 3.0)),
        ))
        .unwrap();
        // Check value is returned correctly
        assert_eq!(rescale_intercept(&dcm), vec![3.0]);
    }

    #[test]
    fn get_required_field_from_shared_fn_groups_improper_placement() {
        let mut dcm = dummy_dicom();
        // Add shared functional groups sequence
        dcm.apply(AttributeOp::new(
            tags::SHARED_FUNCTIONAL_GROUPS_SEQUENCE,
            AttributeAction::SetIfMissing(PrimitiveValue::Empty),
        ))
        .unwrap();
        // Check the fn still returns nothing.
        assert_eq!(rescale_intercept(&dcm), vec![0.0]);

        // Add rescale intercept at top level of SharedFunctionalGroupsSequence
        dcm.apply(AttributeOp::new(
            (
                tags::SHARED_FUNCTIONAL_GROUPS_SEQUENCE,
                0,
                tags::RESCALE_INTERCEPT,
            ),
            AttributeAction::Set(dicom_value!(F64, 3.0)),
        ))
        .unwrap();
        // Check value is returned correctly
        assert_eq!(rescale_intercept(&dcm), vec![3.0]);
    }

    #[test]
    fn get_required_field_from_per_frame_fns() {
        let mut dcm = dummy_dicom();
        let rescale = |v| {
            DataElement::new(
                tags::PIXEL_VALUE_TRANSFORMATION_SEQUENCE,
                VR::SQ,
                DataSetSequence::from(vec![InMemDicomObject::from_element_iter([
                    DataElement::new(tags::RESCALE_INTERCEPT, VR::DS, dicom_value!(F64, v)),
                ])]),
            )
        };

        let exp = vec![1.0, 3.0, 5.0];

        let els = exp
            .iter()
            .map(|v| InMemDicomObject::from_element_iter([rescale(*v)]))
            .collect::<Vec<_>>();

        dcm.put(DataElement::new(
            tags::PER_FRAME_FUNCTIONAL_GROUPS_SEQUENCE,
            VR::SQ,
            DataSetSequence::from(els),
        ));
        assert_eq!(rescale_intercept(&dcm), exp);
    }

    #[test]
    fn get_required_field_with_fallback() {
        let mut dcm = dummy_dicom();

        // Set RescaleIntercept under SharedFunctionalGroupsSequence
        dcm.apply(AttributeOp::new(
            (
                tags::SHARED_FUNCTIONAL_GROUPS_SEQUENCE,
                tags::PIXEL_VALUE_TRANSFORMATION_SEQUENCE,
                tags::RESCALE_INTERCEPT,
            ),
            AttributeAction::Set(dicom_value!(F64, 3.0)),
        ))
        .unwrap();
        // Check the fn returns the correct value
        assert_eq!(rescale_intercept(&dcm), vec![3.0]);

        // Add a PerFrameFunctionalGroupsSequence with no entry for RescaleIntercept
        dcm.put(DataElement::new(
            tags::PER_FRAME_FUNCTIONAL_GROUPS_SEQUENCE,
            VR::SQ,
            DataSetSequence::from(vec![]),
        ));
        // Check the fn still returns the correct value, falling back to SharedFunctionalGroupsSequence
        assert_eq!(rescale_intercept(&dcm), vec![3.0]);
    }
}
