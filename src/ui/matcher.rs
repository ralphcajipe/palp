use quick_xml::events::Event;
use quick_xml::reader::Reader;
use crate::error::PalpError;

#[derive(Debug, Clone)]
pub struct Rect {
    pub left: u32,
    pub top: u32,
    pub right: u32,
    pub bottom: u32,
}

impl Rect {
    pub fn center(&self) -> (u32, u32) {
        ((self.left + self.right) / 2, (self.top + self.bottom) / 2)
    }

    pub fn parse(s: &str) -> Option<Self> {
        // Expected format: "[left,top][right,bottom]" e.g. "[296,1225][540,1498]"
        let s = s.trim();
        let parts: Vec<&str> = s.split("][").collect();
        if parts.len() != 2 {
            return None;
        }

        let p1 = parts[0].trim_start_matches('[');
        let p2 = parts[1].trim_end_matches(']');

        let mut c1 = p1.split(',');
        let mut c2 = p2.split(',');

        let left = c1.next()?.trim().parse::<u32>().ok()?;
        let top = c1.next()?.trim().parse::<u32>().ok()?;
        let right = c2.next()?.trim().parse::<u32>().ok()?;
        let bottom = c2.next()?.trim().parse::<u32>().ok()?;

        Some(Rect {
            left,
            top,
            right,
            bottom,
        })
    }
}

#[derive(Debug, Clone)]
pub struct UiElement {
    pub text: String,
    pub content_desc: String,
    pub resource_id: String,
    pub bounds: Rect,
}

impl UiElement {
    pub fn center(&self) -> (u32, u32) {
        self.bounds.center()
    }
}

/// Finds all elements or a specific element matching a query in the XML hierarchy.
pub fn find_element_by_text(xml: &str, query: &str) -> Result<UiElement, PalpError> {
    let elements = parse_all_elements(xml)?;
    let query_lower = query.trim().to_lowercase();

    // 1. Exact match on text
    if let Some(el) = elements.iter().find(|e| e.text.trim().to_lowercase() == query_lower) {
        return Ok(el.clone());
    }

    // 2. Exact match on content description (accessibility label)
    if let Some(el) = elements.iter().find(|e| e.content_desc.trim().to_lowercase() == query_lower) {
        return Ok(el.clone());
    }

    // 3. Exact match or ending with resource-id (e.g. testID)
    if let Some(el) = elements.iter().find(|e| {
        let res = e.resource_id.trim().to_lowercase();
        res == query_lower || res.ends_with(&format!("/{}", query_lower)) || res.ends_with(&format!(":id/{}", query_lower))
    }) {
        return Ok(el.clone());
    }

    // 4. Substring containment match on text or content description
    if let Some(el) = elements.iter().find(|e| {
        e.text.to_lowercase().contains(&query_lower) || e.content_desc.to_lowercase().contains(&query_lower)
    }) {
        return Ok(el.clone());
    }

    Err(PalpError::TextNotFound(query.to_string()))
}

/// Parses all visible UI elements from a uiautomator XML string.
pub fn parse_all_elements(xml: &str) -> Result<Vec<UiElement>, PalpError> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);

    let mut elements = Vec::new();

    loop {
        match reader.read_event() {
            Ok(Event::Start(ref e)) | Ok(Event::Empty(ref e)) => {
                if e.name().as_ref() == b"node" {
                    let mut text = String::new();
                    let mut content_desc = String::new();
                    let mut resource_id = String::new();
                    let mut bounds_str = String::new();

                    for attr in e.attributes().flatten() {
                        match attr.key.as_ref() {
                            b"text" => {
                                text = String::from_utf8_lossy(&attr.value).to_string();
                            }
                            b"content-desc" => {
                                content_desc = String::from_utf8_lossy(&attr.value).to_string();
                            }
                            b"resource-id" => {
                                resource_id = String::from_utf8_lossy(&attr.value).to_string();
                            }
                            b"bounds" => {
                                bounds_str = String::from_utf8_lossy(&attr.value).to_string();
                            }
                            _ => {}
                        }
                    }

                    if let Some(bounds) = Rect::parse(&bounds_str) {
                        // Only collect elements that have non-zero bounds
                        if bounds.right > bounds.left && bounds.bottom > bounds.top {
                            elements.push(UiElement {
                                text,
                                content_desc,
                                resource_id,
                                bounds,
                            });
                        }
                    }
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => {
                return Err(PalpError::AdbFailed(format!("XML parsing failed: {}", e)));
            }
            _ => {}
        }
    }

    Ok(elements)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rect_parse() {
        let rect = Rect::parse("[296,1225][540,1498]").unwrap();
        assert_eq!(rect.left, 296);
        assert_eq!(rect.top, 1225);
        assert_eq!(rect.right, 540);
        assert_eq!(rect.bottom, 1498);
        assert_eq!(rect.center(), (418, 1361));
    }

    #[test]
    fn test_find_element() {
        let sample = r#"<?xml version='1.0' encoding='UTF-8' standalone='yes' ?><hierarchy><node index="0" text="Submit Order" resource-id="com.app:id/submit" bounds="[100,200][300,400]" /></hierarchy>"#;
        let el = find_element_by_text(sample, "Submit Order").unwrap();
        assert_eq!(el.center(), (200, 300));

        let el_by_id = find_element_by_text(sample, "submit").unwrap();
        assert_eq!(el_by_id.center(), (200, 300));
    }
}
