// noFriction Meetings - Vision OCR Module
// Uses macOS Vision framework for native text recognition from screenshots
//
// Implementation uses objc2 bindings to call Vision's VNRecognizeTextRequest

use chrono::{DateTime, Utc};
use image::DynamicImage;
use serde::{Deserialize, Serialize};
use std::path::Path;

#[cfg(target_os = "macos")]
use std::ffi::c_void;

/// Result of OCR text recognition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OcrResult {
    /// Extracted text content
    pub text: String,
    /// Overall confidence score (0.0 - 1.0)
    pub confidence: f32,
    /// Individual text regions with bounding boxes
    pub regions: Vec<TextRegion>,
    /// Processing duration in milliseconds
    pub duration_ms: u64,
    /// Extraction timestamp
    pub extracted_at: DateTime<Utc>,
    /// Method used for extraction
    pub method: ExtractionMethod,
}

impl Default for OcrResult {
    fn default() -> Self {
        Self {
            text: String::new(),
            confidence: 0.0,
            regions: Vec::new(),
            duration_ms: 0,
            extracted_at: Utc::now(),
            method: ExtractionMethod::None,
        }
    }
}

/// Individual text region with location
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextRegion {
    /// Recognized text in this region
    pub text: String,
    /// Confidence for this region (0.0 - 1.0)
    pub confidence: f32,
    /// Normalized bounding box (0.0 - 1.0 coordinates)
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

/// Extraction method used
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExtractionMethod {
    VisionOcr,
    Accessibility,
    Fallback,
    None,
}

/// Configuration for Vision OCR
#[derive(Debug, Clone)]
pub struct VisionOcrConfig {
    /// Recognition level: true = accurate (slower), false = fast
    pub accurate_mode: bool,
    /// Minimum text height as fraction of image height
    pub min_text_height: f32,
    /// Languages to recognize (empty = auto-detect)
    pub languages: Vec<String>,
    /// Use language correction
    pub use_language_correction: bool,
}

impl Default for VisionOcrConfig {
    fn default() -> Self {
        Self {
            accurate_mode: true,
            min_text_height: 0.0,
            languages: vec!["en-US".to_string()],
            use_language_correction: true,
        }
    }
}

/// Vision OCR engine for macOS
pub struct VisionOcr {
    config: VisionOcrConfig,
}

impl VisionOcr {
    pub fn new() -> Self {
        Self::with_config(VisionOcrConfig::default())
    }

    pub fn with_config(config: VisionOcrConfig) -> Self {
        Self { config }
    }

    /// Recognize text from a DynamicImage
    #[cfg(target_os = "macos")]
    pub fn recognize_text(&self, image: &DynamicImage) -> Result<OcrResult, String> {
        let start = std::time::Instant::now();

        // Convert DynamicImage to PNG bytes
        let mut png_bytes = Vec::new();
        let mut cursor = std::io::Cursor::new(&mut png_bytes);
        image
            .write_to(&mut cursor, image::ImageFormat::Png)
            .map_err(|e| format!("Failed to encode image: {}", e))?;

        // Call native Vision framework
        let result = self.recognize_from_bytes(&png_bytes)?;

        let duration_ms = start.elapsed().as_millis() as u64;

        Ok(OcrResult {
            text: result.text,
            confidence: result.confidence,
            regions: result.regions,
            duration_ms,
            extracted_at: Utc::now(),
            method: ExtractionMethod::VisionOcr,
        })
    }

    /// Recognize text from image file path
    #[cfg(target_os = "macos")]
    pub fn recognize_from_file(&self, path: &Path) -> Result<OcrResult, String> {
        let image = image::open(path).map_err(|e| format!("Failed to open image: {}", e))?;
        self.recognize_text(&image)
    }

    /// Core recognition using Vision framework via objc
    #[cfg(target_os = "macos")]
    fn recognize_from_bytes(&self, image_bytes: &[u8]) -> Result<OcrResultInternal, String> {
        use objc::runtime::{Class, Object, Sel, BOOL, YES};
        use objc::{msg_send, sel, sel_impl};
        use std::ptr;

        unsafe {
            // Create NSData from bytes
            let nsdata_class = Class::get("NSData").ok_or("NSData class not found")?;
            let nsdata: *mut Object = msg_send![nsdata_class, alloc];
            let nsdata: *mut Object =
                msg_send![nsdata, initWithBytes:image_bytes.as_ptr() length:image_bytes.len()];
            if nsdata.is_null() {
                return Err("Failed to create NSData".to_string());
            }

            // Create CIImage from NSData, then get CGImage
            // This is more robust than using core_graphics directly
            let ciimage_class = Class::get("CIImage").ok_or("CIImage class not found")?;
            let ciimage: *mut Object = msg_send![ciimage_class, imageWithData: nsdata];
            if ciimage.is_null() {
                return Err("Failed to create CIImage from data".to_string());
            }

            // Get CGImage from CIImage
            let cicontext_class = Class::get("CIContext").ok_or("CIContext class not found")?;
            let context: *mut Object = msg_send![cicontext_class, context];

            // Get extent of CIImage
            let extent: core_graphics::geometry::CGRect = msg_send![ciimage, extent];

            // Create CGImage from CIImage
            let cg_image_ptr: *mut c_void =
                msg_send![context, createCGImage:ciimage fromRect:extent];
            if cg_image_ptr.is_null() {
                return Err("Failed to create CGImage from CIImage".to_string());
            }

            // Create VNImageRequestHandler with CGImage
            let handler_class =
                Class::get("VNImageRequestHandler").ok_or("VNImageRequestHandler not found")?;
            let handler: *mut Object = msg_send![handler_class, alloc];
            let handler: *mut Object =
                msg_send![handler, initWithCGImage:cg_image_ptr options:ptr::null::<Object>()];
            if handler.is_null() {
                return Err("Failed to create VNImageRequestHandler".to_string());
            }

            // Create VNRecognizeTextRequest
            let request_class =
                Class::get("VNRecognizeTextRequest").ok_or("VNRecognizeTextRequest not found")?;
            let request: *mut Object = msg_send![request_class, alloc];
            let request: *mut Object = msg_send![request, init];
            if request.is_null() {
                return Err("Failed to create VNRecognizeTextRequest".to_string());
            }

            // Configure request
            let recognition_level: i64 = if self.config.accurate_mode { 1 } else { 0 }; // 1 = accurate, 0 = fast
            let _: () = msg_send![request, setRecognitionLevel: recognition_level];
            let _: () = msg_send![request, setUsesLanguageCorrection: self.config.use_language_correction as BOOL];

            // Set recognition languages if specified
            if !self.config.languages.is_empty() {
                let nsarray_class =
                    Class::get("NSMutableArray").ok_or("NSMutableArray not found")?;
                let languages_array: *mut Object = msg_send![nsarray_class, array];

                for lang in &self.config.languages {
                    let nsstring_class = Class::get("NSString").ok_or("NSString not found")?;
                    let lang_str: *mut Object =
                        msg_send![nsstring_class, stringWithUTF8String: lang.as_ptr()];
                    let _: () = msg_send![languages_array, addObject: lang_str];
                }

                let _: () = msg_send![request, setRecognitionLanguages: languages_array];
            }

            // Create requests array
            let nsarray_class = Class::get("NSArray").ok_or("NSArray not found")?;
            let requests: *mut Object = msg_send![nsarray_class, arrayWithObject: request];

            // Perform request
            let mut error: *mut Object = ptr::null_mut();
            let success: BOOL = msg_send![handler, performRequests:requests error:&mut error];

            if success != YES {
                let error_desc = if !error.is_null() {
                    let desc: *mut Object = msg_send![error, localizedDescription];
                    nsstring_to_rust(desc)
                } else {
                    "Unknown Vision error".to_string()
                };
                return Err(format!("Vision request failed: {}", error_desc));
            }

            // Get results
            let results: *mut Object = msg_send![request, results];
            if results.is_null() {
                return Ok(OcrResultInternal {
                    text: String::new(),
                    confidence: 0.0,
                    regions: Vec::new(),
                });
            }

            // Process results
            let count: usize = msg_send![results, count];
            let mut all_text = Vec::new();
            let mut all_confidence = 0.0f32;
            let mut regions = Vec::new();

            for i in 0..count {
                let observation: *mut Object = msg_send![results, objectAtIndex: i];
                if observation.is_null() {
                    continue;
                }

                // Get top candidate
                let candidates: *mut Object = msg_send![observation, topCandidates: 1usize];
                if candidates.is_null() {
                    continue;
                }

                let candidate_count: usize = msg_send![candidates, count];
                if candidate_count == 0 {
                    continue;
                }

                let candidate: *mut Object = msg_send![candidates, objectAtIndex: 0usize];
                if candidate.is_null() {
                    continue;
                }

                // Get text and confidence
                let text_obj: *mut Object = msg_send![candidate, string];
                let text = nsstring_to_rust(text_obj);
                let confidence: f32 = msg_send![candidate, confidence];

                if !text.is_empty() {
                    all_text.push(text.clone());
                    all_confidence += confidence;

                    // Get bounding box (normalized coordinates)
                    // VNTextObservation has boundingBox property
                    let bbox: core_graphics::geometry::CGRect = msg_send![observation, boundingBox];

                    regions.push(TextRegion {
                        text,
                        confidence,
                        x: bbox.origin.x as f32,
                        y: bbox.origin.y as f32,
                        width: bbox.size.width as f32,
                        height: bbox.size.height as f32,
                    });
                }
            }

            // Calculate average confidence
            let avg_confidence = if !regions.is_empty() {
                all_confidence / regions.len() as f32
            } else {
                0.0
            };

            // Join all text with newlines
            let full_text = all_text.join("\n");

            Ok(OcrResultInternal {
                text: full_text,
                confidence: avg_confidence,
                regions,
            })
        }
    }

    /// Non-macOS stub
    #[cfg(not(target_os = "macos"))]
    pub fn recognize_text(&self, _image: &DynamicImage) -> Result<OcrResult, String> {
        Err("Vision OCR only available on macOS".to_string())
    }

    #[cfg(not(target_os = "macos"))]
    pub fn recognize_from_file(&self, _path: &Path) -> Result<OcrResult, String> {
        Err("Vision OCR only available on macOS".to_string())
    }
}

impl Default for VisionOcr {
    fn default() -> Self {
        Self::new()
    }
}

/// Internal result structure
struct OcrResultInternal {
    text: String,
    confidence: f32,
    regions: Vec<TextRegion>,
}

/// Helper to convert NSString to Rust String
/// Also handles NSNumber by converting to string representation
#[cfg(target_os = "macos")]
unsafe fn nsstring_to_rust(nsstring: *mut objc::runtime::Object) -> String {
    use objc::{class, msg_send, sel, sel_impl};
    use std::ffi::CStr;

    if nsstring.is_null() {
        return String::new();
    }

    // Check if this is actually an NSString (or subclass)
    let nsstring_class = class!(NSString);
    let is_nsstring: bool = msg_send![nsstring, isKindOfClass: nsstring_class];

    if is_nsstring {
        let utf8: *const i8 = msg_send![nsstring, UTF8String];
        if utf8.is_null() {
            return String::new();
        }
        return CStr::from_ptr(utf8).to_str().unwrap_or("").to_string();
    }

    // Check if this is an NSNumber - convert to string representation
    let nsnumber_class = class!(NSNumber);
    let is_nsnumber: bool = msg_send![nsstring, isKindOfClass: nsnumber_class];

    if is_nsnumber {
        let description: *mut objc::runtime::Object = msg_send![nsstring, stringValue];
        if !description.is_null() {
            let utf8: *const i8 = msg_send![description, UTF8String];
            if !utf8.is_null() {
                return CStr::from_ptr(utf8).to_str().unwrap_or("").to_string();
            }
        }
        return String::new();
    }

    // Fallback: try description
    let description: *mut objc::runtime::Object = msg_send![nsstring, description];
    if !description.is_null() {
        let utf8: *const i8 = msg_send![description, UTF8String];
        if !utf8.is_null() {
            return CStr::from_ptr(utf8).to_str().unwrap_or("").to_string();
        }
    }

    String::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ocr_config_defaults() {
        let config = VisionOcrConfig::default();
        assert!(config.accurate_mode);
        assert!(config.use_language_correction);
        assert!(!config.languages.is_empty());
    }

    #[test]
    fn test_ocr_result_default() {
        let result = OcrResult::default();
        assert!(result.text.is_empty());
        assert_eq!(result.confidence, 0.0);
        assert!(result.regions.is_empty());
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn test_vision_ocr_initialization() {
        let ocr = VisionOcr::new();
        assert!(ocr.config.accurate_mode);
    }
}
