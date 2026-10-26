use quick_xml::events::{BytesStart, Event};
use quick_xml::reader::Reader;

use crate::error::PalpError;

#[derive(Debug, Clone, PartialEq, Eq)]
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

    /// Off-screen rows come back with zero height; they cannot be tapped or seen.
    pub fn has_area(&self) -> bool {
        self.right > self.left && self.bottom > self.top
    }

    pub fn contains(&self, (x, y): (u32, u32)) -> bool {
        x >= self.left && x < self.right && y >= self.top && y < self.bottom
    }
}

#[derive(Debug, Clone)]
pub struct UiElement {
    pub text: String,
    pub content_desc: String,
    pub resource_id: String,
    pub class: String,
    pub clickable: bool,
    pub bounds: Rect,
}

impl UiElement {
    pub fn center(&self) -> (u32, u32) {
        self.bounds.center()
    }

    /// What a person would call this element: its text, else its accessibility label, else its ID.
    pub fn label(&self) -> &str {
        [&self.text, &self.content_desc, &self.resource_id]
            .into_iter()
            .find(|s| !s.trim().is_empty())
            .map_or("", |s| s.as_str())
    }

    /// Native, Compose, Flutter and WebView text inputs all report this class.
    pub fn is_field(&self) -> bool {
        self.class.ends_with("EditText")
    }
}

/// One `uiautomator dump`: every node in dump order, which is also drawing order (a later
/// node is drawn over an earlier one), with its parent so overlap can ignore containers.
pub struct Screen {
    elements: Vec<UiElement>,
    parents: Vec<Option<usize>>,
}

impl Screen {
    pub fn element(&self, index: usize) -> &UiElement {
        &self.elements[index]
    }

    /// Indexes of nodes that are on screen, in dump order.
    pub fn candidates(&self) -> impl Iterator<Item = usize> + '_ {
        (0..self.elements.len()).filter(|&i| self.elements[i].bounds.has_area())
    }

    fn is_ancestor(&self, ancestor: usize, mut node: usize) -> bool {
        while let Some(parent) = self.parents[node] {
            if parent == ancestor {
                return true;
            }
            node = parent;
        }
        false
    }

    /// True when something drawn later, other than the node's own container or content,
    /// lies over its centre: a dialog, a sheet or a floating bar. A tap there would hit that.
    pub fn is_covered(&self, index: usize) -> bool {
        let center = self.elements[index].center();
        (index + 1..self.elements.len()).any(|later| {
            let bounds = &self.elements[later].bounds;
            bounds.has_area()
                && bounds.contains(center)
                && !self.is_ancestor(later, index)
                && !self.is_ancestor(index, later)
        })
    }

    /// Every on-screen node in the best matching tier: exact text, exact accessibility
    /// label, resource ID, then (unless `exact`) any text or label containing the query.
    pub fn find_matches(&self, query: &str, exact: bool) -> Vec<usize> {
        let q = query.trim().to_lowercase();
        let tiers: [&dyn Fn(&UiElement) -> bool; 4] = [
            &|e| e.text.trim().to_lowercase() == q,
            &|e| e.content_desc.trim().to_lowercase() == q,
            &|e| {
                let id = e.resource_id.trim().to_lowercase();
                id == q || id.ends_with(&format!("/{q}")) || id.ends_with(&format!(":id/{q}"))
            },
            &|e| e.text.to_lowercase().contains(&q) || e.content_desc.to_lowercase().contains(&q),
        ];
        let usable = if exact { &tiers[..3] } else { &tiers[..] };
        for tier in usable {
            let found: Vec<usize> = self
                .candidates()
                .filter(|&i| tier(&self.elements[i]))
                .collect();
            if !found.is_empty() {
                return found;
            }
        }
        Vec::new()
    }

    /// Sorted the way a person reads the screen: top to bottom, then left to right.
    pub fn in_reading_order(&self, mut indexes: Vec<usize>) -> Vec<usize> {
        indexes.sort_by_key(|&i| {
            let b = &self.elements[i].bounds;
            (b.top, b.left)
        });
        indexes
    }
}

/// Why `pick` chose the element it did, for the message shown when there were several.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PickRule {
    OnlyMatch,
    Nth,
    NotCovered,
    Tappable,
    Topmost,
}

impl PickRule {
    pub fn describe(self) -> &'static str {
        match self {
            PickRule::OnlyMatch => "the only match",
            PickRule::Nth => "the one asked for with --nth",
            PickRule::NotCovered => "the one not under a dialog or bar",
            PickRule::Tappable => "the tappable one",
            PickRule::Topmost => "the first from the top",
        }
    }
}

pub struct Pick {
    pub chosen: usize,
    /// Every match, in reading order, for the list printed when there was a choice.
    pub matches: Vec<usize>,
    pub rule: PickRule,
}

/// Chooses which element a tap on `query` means.
///
/// Matches under a dialog or bar are dropped, because a tap at their centre lands on the
/// cover. Then tappable ones win over plain text (a tab over a page title with the same
/// words), then the first from the top. `nth` overrides all of that, counting every match
/// from the top.
pub fn pick(
    screen: &Screen,
    query: &str,
    exact: bool,
    nth: Option<usize>,
) -> Result<Pick, PalpError> {
    if query.trim().is_empty() {
        return Err(PalpError::InvalidInput("Give the text to look for.".into()));
    }
    let matches = screen.in_reading_order(screen.find_matches(query, exact));
    if matches.is_empty() {
        return Err(PalpError::TextNotFound(query.to_string()));
    }

    if let Some(n) = nth {
        let chosen = n
            .checked_sub(1)
            .and_then(|i| matches.get(i))
            .copied()
            .ok_or_else(|| {
                PalpError::InvalidInput(format!(
                    "--nth {n} asked for, but '{query}' matches {} element(s) on screen.",
                    matches.len()
                ))
            })?;
        return Ok(Pick {
            chosen,
            matches,
            rule: PickRule::Nth,
        });
    }
    if matches.len() == 1 {
        return Ok(Pick {
            chosen: matches[0],
            matches,
            rule: PickRule::OnlyMatch,
        });
    }

    let uncovered: Vec<usize> = matches
        .iter()
        .copied()
        .filter(|&i| !screen.is_covered(i))
        .collect();
    let (pool, mut rule) = if uncovered.is_empty() || uncovered.len() == matches.len() {
        (matches.clone(), PickRule::Topmost)
    } else {
        (uncovered, PickRule::NotCovered)
    };
    let tappable: Vec<usize> = pool
        .iter()
        .copied()
        .filter(|&i| screen.element(i).clickable)
        .collect();
    let pool = if !tappable.is_empty() && tappable.len() < pool.len() {
        rule = PickRule::Tappable;
        tappable
    } else {
        pool
    };
    if pool.len() == 1 && rule == PickRule::Topmost {
        rule = PickRule::OnlyMatch;
    }
    Ok(Pick {
        chosen: pool[0],
        matches,
        rule,
    })
}

/// Parses a `uiautomator dump`, keeping every node (even off-screen ones) so containers can
/// be recognised, with attribute values unescaped: the dump writes `&` as `&amp;` and
/// emoji as `&#127477;`, which never matched what is on screen.
pub fn parse_screen(xml: &str) -> Result<Screen, PalpError> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    let mut elements = Vec::new();
    let mut parents = Vec::new();
    let mut open: Vec<usize> = Vec::new();

    loop {
        match reader.read_event() {
            Ok(Event::Start(ref e)) if e.name().as_ref() == b"node" => {
                parents.push(open.last().copied());
                elements.push(element_from(e));
                open.push(elements.len() - 1);
            }
            Ok(Event::Empty(ref e)) if e.name().as_ref() == b"node" => {
                parents.push(open.last().copied());
                elements.push(element_from(e));
            }
            Ok(Event::End(ref e)) if e.name().as_ref() == b"node" => {
                open.pop();
            }
            Ok(Event::Eof) => break,
            Err(e) => {
                return Err(PalpError::AdbFailed(format!(
                    "Failed to parse UI XML at position {}: {:?}",
                    reader.error_position(),
                    e
                )));
            }
            _ => {}
        }
    }

    Ok(Screen { elements, parents })
}

fn element_from(e: &BytesStart) -> UiElement {
    let mut el = UiElement {
        text: String::new(),
        content_desc: String::new(),
        resource_id: String::new(),
        class: String::new(),
        clickable: false,
        bounds: Rect {
            left: 0,
            top: 0,
            right: 0,
            bottom: 0,
        },
    };
    for attr in e.attributes().flatten() {
        let value = attr
            .unescape_value()
            .map(|v| v.into_owned())
            .unwrap_or_else(|_| String::from_utf8_lossy(&attr.value).into_owned());
        match attr.key.as_ref() {
            b"text" => el.text = value,
            b"content-desc" => el.content_desc = value,
            b"resource-id" => el.resource_id = value,
            b"class" => el.class = value,
            b"clickable" => el.clickable = value == "true",
            b"bounds" => {
                if let Some(bounds) = Rect::parse(&value) {
                    el.bounds = bounds;
                }
            }
            _ => {}
        }
    }
    el
}

#[cfg(test)]
mod tests {
    use super::*;

    const SETTINGS: &str = include_str!("fixtures/settings_page.xml");
    const DIALOG: &str = include_str!("fixtures/dialog_over_page.xml");
    const LIST: &str = include_str!("fixtures/fullscreen_list.xml");

    fn picked<'a>(
        screen: &'a Screen,
        query: &str,
        exact: bool,
        nth: Option<usize>,
    ) -> &'a UiElement {
        let pick = pick(screen, query, exact, nth).unwrap();
        screen.element(pick.chosen)
    }

    fn index_of(screen: &Screen, text: &str) -> usize {
        screen
            .candidates()
            .find(|&i| screen.element(i).text == text)
            .unwrap()
    }

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
        let screen = parse_screen(sample).unwrap();
        assert_eq!(
            picked(&screen, "Submit Order", false, None).center(),
            (200, 300)
        );
        assert_eq!(picked(&screen, "submit", false, None).center(), (200, 300));
    }

    #[test]
    fn a_dialog_wins_over_the_row_drawn_behind_it() {
        let screen = parse_screen(DIALOG).unwrap();
        let row = index_of(
            &screen,
            "Country Philippines · Pro Bono asks for a certificate",
        );
        assert!(screen.is_covered(row));

        let chosen = picked(&screen, "Philippines", false, None);
        assert_eq!(chosen.content_desc, "Country: Philippines. Change");
    }

    #[test]
    fn a_full_screen_list_wins_over_the_page_behind_it() {
        let screen = parse_screen(LIST).unwrap();
        let chosen = picked(&screen, "Philippines", false, None);
        assert_eq!(chosen.text, "🇵🇭 Philippines");
        assert_eq!(chosen.center(), (539, 1395));
    }

    #[test]
    fn a_tap_prefers_the_tappable_match_and_nth_counts_from_the_top() {
        let screen = parse_screen(SETTINGS).unwrap();
        let tab = picked(&screen, "Settings", false, None);
        assert_eq!(
            (tab.class.as_str(), tab.center()),
            ("android.widget.Button", (973, 2244))
        );

        let title = picked(&screen, "Settings", false, Some(1));
        assert_eq!(title.class, "android.widget.TextView");
        assert_eq!(
            picked(&screen, "Settings", false, Some(2)).center(),
            (973, 2244)
        );
        assert!(matches!(
            pick(&screen, "Settings", false, Some(3)),
            Err(PalpError::InvalidInput(_))
        ));
    }

    #[test]
    fn the_upper_of_two_equal_buttons_is_chosen() {
        let xml = r#"<hierarchy>
            <node text="Delete" class="android.widget.Button" clickable="true" bounds="[0,900][500,1000]" />
            <node text="Delete" class="android.widget.Button" clickable="true" bounds="[0,300][500,400]" />
        </hierarchy>"#;
        let screen = parse_screen(xml).unwrap();
        assert_eq!(picked(&screen, "Delete", false, None).center(), (250, 350));
    }

    #[test]
    fn a_covered_element_is_still_found_when_it_is_the_only_match() {
        let screen = parse_screen(SETTINGS).unwrap();
        let status = index_of(&screen, "Service status All systems normal");
        assert!(screen.is_covered(status), "the bottom bar is drawn over it");
        assert_eq!(
            picked(&screen, "Service status", false, None).center(),
            (539, 2268)
        );
    }

    #[test]
    fn escaped_text_matches_what_the_user_sees() {
        let screen = parse_screen(SETTINGS).unwrap();
        assert_eq!(
            picked(&screen, "Share & connect", true, None).text,
            "Share & connect"
        );
    }

    #[test]
    fn exact_ignores_partial_matches() {
        let screen = parse_screen(DIALOG).unwrap();
        assert!(matches!(
            pick(&screen, "Philippines", true, None),
            Err(PalpError::TextNotFound(_))
        ));
    }

    #[test]
    fn nothing_matching_and_an_empty_query_are_errors() {
        let screen = parse_screen(SETTINGS).unwrap();
        assert!(matches!(
            pick(&screen, "Checkout", false, None),
            Err(PalpError::TextNotFound(_))
        ));
        assert!(matches!(
            pick(&screen, "  ", false, None),
            Err(PalpError::InvalidInput(_))
        ));
    }

    #[test]
    fn zero_height_rows_scrolled_out_of_view_are_never_matched() {
        let screen = parse_screen(LIST).unwrap();
        assert!(matches!(
            pick(&screen, "Zimbabwe", false, None),
            Err(PalpError::TextNotFound(_))
        ));
    }
}
