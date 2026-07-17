// Taskwarrior-style quick-capture parsing for the "Add Moment" title input
// (see DESIGN_PROGRESS.md). Pure, no I/O — tokenize() and parse() take the
// raw text plus the current entity list and return structured data; the
// component in components/moment.rs owns rendering the colored overlay and
// the @-mention dropdown on top of this.
//
// Recognized syntax:
//   @<entity name>       entity chooser — must match a real entity's full
//                        name, case-insensitively, else it's left as plain
//                        text (this is what the UI treats as "activated")
//   @"<entity name>"     same, but the name is delimited by double quotes
//                        instead of ending at the next word boundary — lets
//                        the live @-mention search span multiple words
//                        without needing an exact prefix match first (see
//                        trailing_mention_query)
//   priority:H/M/L       (also accepts high/medium/low) — pri: is a shorthand alias
//   project:<value>      no spaces — stops at the next whitespace — pro: is a shorthand alias
//   due:<date>           "today", "tomorrow", "YYYY-MM-DD", or "YYYY-MM-DDTHH:MM"
//   scheduled:<date>     same date grammar as due — hides the moment from
//                        the normal views until this date (see the
//                        Scheduled view). wait: is an alias (taskwarrior's
//                        own name for this).
//   until:<date>         same date grammar as due
//   depends:<title>      blocked by another open moment, matched by title —
//                        no spaces (stops at whitespace) unless quoted, same
//                        as tags/mentions below. deps: is a shorthand alias.
//                        Resolved against the target entity's open moments
//                        at submit time (this module has no moments list to
//                        search — see ParsedCapture.depends_on_title).
//   +<tag> / -<tag>      add / remove a tag (single word)
//   +"<tag>" / -"<tag>"  add / remove a tag containing spaces
//   ;t; / ;p; / ;n;      set the moment's type: task / promise / note —
//                        home-row, stands alone as its own word
//
// Anything that isn't recognized (including a malformed date, an
// unrecognized @name, or an unterminated quote) is left as plain text and
// becomes part of the title — that's the signal to the user that it didn't
// "activate".
use crate::types::EntityType;
use chrono::{Duration, NaiveDate, NaiveDateTime};

#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    Plain,
    Priority(String),
    Project(String),
    Due(String),
    Scheduled(String),
    Until(String),
    TagAdd(String),
    TagRemove(String),
    Entity(String),
    Depends(String),
    MomentType(i64),
}

impl TokenKind {
    pub fn is_recognized(&self) -> bool {
        !matches!(self, TokenKind::Plain)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub text: String,
    pub kind: TokenKind,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ParsedCapture {
    pub title: String,
    pub entity_id: Option<String>,
    pub priority: Option<String>,
    pub project: Option<String>,
    pub due_at: Option<String>,
    pub scheduled_at: Option<String>,
    pub until_at: Option<String>,
    pub tags_add: Vec<String>,
    pub tags_remove: Vec<String>,
    // Raw typed text, not yet resolved to a moment id — this module has no
    // moments list to search titles against (deliberately pure/no-I/O, see
    // module doc comment). The caller resolves this against the target
    // entity's open moments at submit time.
    pub depends_on_title: Option<String>,
    pub moment_type_id: Option<i64>,
}

impl ParsedCapture {
    pub fn has_metadata(&self) -> bool {
        self.priority.is_some()
            || self.project.is_some()
            || self.scheduled_at.is_some()
            || self.until_at.is_some()
            || !self.tags_add.is_empty()
    }
}

// Finds the longest entity name (case-insensitive) that is a prefix of
// `rest` and ends at a word boundary (whitespace or end of string) — so
// "@Jan" doesn't falsely match an entity named "Jane", and if both "Jane"
// and "Jane Doe" exist, the longer name wins.
fn match_entity<'a>(rest: &str, entities: &'a [EntityType]) -> Option<(&'a EntityType, usize)> {
    let rest_lower = rest.to_lowercase();
    let mut best: Option<(&EntityType, usize)> = None;
    for entity in entities {
        if entity.name.is_empty() {
            continue;
        }
        let name_lower = entity.name.to_lowercase();
        if !rest_lower.starts_with(&name_lower) {
            continue;
        }
        let byte_len = entity.name.len();
        let boundary_ok = match rest.get(byte_len..).and_then(|s| s.chars().next()) {
            None => true,
            Some(c) => c.is_whitespace(),
        };
        if boundary_ok && best.map(|(_, l)| byte_len > l).unwrap_or(true) {
            best = Some((entity, byte_len));
        }
    }
    best
}

// `quote_byte_pos` is the byte index of the opening '"'. Returns the inner
// text (unescaped — no escape syntax is supported) and the byte offset just
// past the closing '"', or None if the quote is never closed (still being
// typed, or just malformed — either way it's left for the caller to fall
// back to plain-word tokenization).
fn read_quoted(input: &str, quote_byte_pos: usize) -> Option<(String, usize)> {
    let after_quote = quote_byte_pos + '"'.len_utf8();
    let rest = &input[after_quote..];
    rest.find('"').map(|rel| (rest[..rel].to_string(), after_quote + rel + '"'.len_utf8()))
}

fn normalize_priority(val: &str) -> Option<String> {
    match val.to_lowercase().as_str() {
        "h" | "high" => Some("H".to_string()),
        "m" | "medium" => Some("M".to_string()),
        "l" | "low" => Some("L".to_string()),
        _ => None,
    }
}

fn parse_date_token(val: &str) -> Option<String> {
    let today = chrono::Utc::now().date_naive();
    match val.to_lowercase().as_str() {
        "today" => return Some(format!("{}T00:00", today.format("%Y-%m-%d"))),
        "tomorrow" => {
            let d = today + Duration::days(1);
            return Some(format!("{}T00:00", d.format("%Y-%m-%d")));
        }
        _ => {}
    }
    if let Ok(dt) = NaiveDateTime::parse_from_str(val, "%Y-%m-%dT%H:%M") {
        return Some(dt.format("%Y-%m-%dT%H:%M").to_string());
    }
    if let Ok(d) = NaiveDate::parse_from_str(val, "%Y-%m-%d") {
        return Some(format!("{}T00:00", d.format("%Y-%m-%d")));
    }
    None
}

fn classify_word(word: &str) -> TokenKind {
    // Semicolons, not colons — deliberately, sits right on the home row
    // with no shift needed, and doesn't visually collide with due:/
    // scheduled:/until:'s colon-heavy date syntax the way :t:/:p:/:n:
    // would have.
    if word == ";t;" {
        return TokenKind::MomentType(1);
    }
    if word == ";p;" {
        return TokenKind::MomentType(2);
    }
    if word == ";n;" {
        return TokenKind::MomentType(3);
    }
    if let Some(rest) = word.strip_prefix('+') {
        if !rest.is_empty() {
            return TokenKind::TagAdd(rest.to_string());
        }
    }
    if let Some(rest) = word.strip_prefix('-') {
        if !rest.is_empty() {
            return TokenKind::TagRemove(rest.to_string());
        }
    }
    if let Some((key, val)) = word.split_once(':') {
        if !val.is_empty() {
            match key.to_lowercase().as_str() {
                "priority" | "pri" => {
                    if let Some(p) = normalize_priority(val) {
                        return TokenKind::Priority(p);
                    }
                }
                "project" | "pro" => return TokenKind::Project(val.to_string()),
                "due" => {
                    if let Some(d) = parse_date_token(val) {
                        return TokenKind::Due(d);
                    }
                }
                // wait: is taskwarrior's own name for this — same field,
                // same syntax, just the more familiar word for what it
                // actually does (hide until this date).
                "scheduled" | "wait" => {
                    if let Some(d) = parse_date_token(val) {
                        return TokenKind::Scheduled(d);
                    }
                }
                "until" => {
                    if let Some(d) = parse_date_token(val) {
                        return TokenKind::Until(d);
                    }
                }
                "depends" | "deps" => return TokenKind::Depends(val.to_string()),
                _ => {}
            }
        }
    }
    TokenKind::Plain
}

// Covers the whole input with no gaps (whitespace included as Plain
// tokens) so a caller can reconstruct the exact string for a highlight
// overlay just by rendering the tokens back to back.
pub fn tokenize(input: &str, entities: &[EntityType]) -> Vec<Token> {
    let mut tokens = Vec::new();
    let chars: Vec<(usize, char)> = input.char_indices().collect();
    let len = input.len();
    let mut i = 0usize;
    'outer: while i < chars.len() {
        let (byte_pos, ch) = chars[i];
        if ch.is_whitespace() {
            let mut j = i;
            while j < chars.len() && chars[j].1.is_whitespace() {
                j += 1;
            }
            let end = chars.get(j).map(|&(p, _)| p).unwrap_or(len);
            tokens.push(Token { text: input[byte_pos..end].to_string(), kind: TokenKind::Plain });
            i = j;
            continue;
        }
        if ch == '@' {
            let after_at = byte_pos + ch.len_utf8();
            if input[after_at..].starts_with('"') {
                if let Some((inner, end)) = read_quoted(input, after_at) {
                    let kind = entities.iter()
                        .find(|e| !e.name.is_empty() && e.name.to_lowercase() == inner.to_lowercase())
                        .map(|e| TokenKind::Entity(e.id.clone()))
                        .unwrap_or(TokenKind::Plain);
                    tokens.push(Token { text: input[byte_pos..end].to_string(), kind });
                    let mut j = i;
                    while j < chars.len() && chars[j].0 < end {
                        j += 1;
                    }
                    i = j;
                    continue;
                }
                // Unterminated quote (still being typed) — fall through to
                // plain word tokenization below; harmless since it just
                // renders uncolored until the quote is closed.
            } else {
                let rest = &input[after_at..];
                if let Some((entity, matched_len)) = match_entity(rest, entities) {
                    let end = after_at + matched_len;
                    tokens.push(Token {
                        text: input[byte_pos..end].to_string(),
                        kind: TokenKind::Entity(entity.id.clone()),
                    });
                    let mut j = i;
                    while j < chars.len() && chars[j].0 < end {
                        j += 1;
                    }
                    i = j;
                    continue;
                }
            }
        }
        // depends:"..."/deps:"..." — same quoted-value trick as @"..." and
        // +"..."/-"...", needed because classify_word only ever sees one
        // whitespace-delimited word at a time and can't span a quoted
        // multi-word title on its own.
        if let Some(key) = ["depends:\"", "deps:\""].iter().find(|k| input[byte_pos..].starts_with(**k)) {
            let after_key = byte_pos + key.len() - 1; // position of the opening quote itself
            if let Some((inner, end)) = read_quoted(input, after_key) {
                let kind = if inner.is_empty() { TokenKind::Plain } else { TokenKind::Depends(inner) };
                tokens.push(Token { text: input[byte_pos..end].to_string(), kind });
                let mut j = i;
                while j < chars.len() && chars[j].0 < end {
                    j += 1;
                }
                i = j;
                continue 'outer;
            }
        }
        if ch == '+' || ch == '-' {
            let after = byte_pos + ch.len_utf8();
            if input[after..].starts_with('"') {
                if let Some((inner, end)) = read_quoted(input, after) {
                    let kind = if inner.is_empty() {
                        TokenKind::Plain
                    } else if ch == '+' {
                        TokenKind::TagAdd(inner)
                    } else {
                        TokenKind::TagRemove(inner)
                    };
                    tokens.push(Token { text: input[byte_pos..end].to_string(), kind });
                    let mut j = i;
                    while j < chars.len() && chars[j].0 < end {
                        j += 1;
                    }
                    i = j;
                    continue;
                }
            }
        }
        let mut j = i;
        while j < chars.len() && !chars[j].1.is_whitespace() {
            j += 1;
        }
        let end = chars.get(j).map(|&(p, _)| p).unwrap_or(len);
        let word = &input[byte_pos..end];
        tokens.push(Token { text: word.to_string(), kind: classify_word(word) });
        i = j;
    }
    tokens
}

pub fn parse(input: &str, entities: &[EntityType]) -> ParsedCapture {
    let tokens = tokenize(input, entities);
    let mut cap = ParsedCapture::default();
    let mut title_parts = Vec::new();
    for t in &tokens {
        match &t.kind {
            TokenKind::Plain => {
                if !t.text.trim().is_empty() {
                    title_parts.push(t.text.as_str());
                }
            }
            TokenKind::Priority(p) => cap.priority = Some(p.clone()),
            TokenKind::Project(p) => cap.project = Some(p.clone()),
            TokenKind::Due(d) => cap.due_at = Some(d.clone()),
            TokenKind::Scheduled(d) => cap.scheduled_at = Some(d.clone()),
            TokenKind::Until(d) => cap.until_at = Some(d.clone()),
            TokenKind::TagAdd(tag) => cap.tags_add.push(tag.clone()),
            TokenKind::TagRemove(tag) => cap.tags_remove.push(tag.clone()),
            TokenKind::Entity(id) => cap.entity_id = Some(id.clone()),
            TokenKind::Depends(title) => cap.depends_on_title = Some(title.clone()),
            TokenKind::MomentType(id) => cap.moment_type_id = Some(*id),
        }
    }
    cap.title = title_parts.join(" ");
    cap
}

#[derive(Debug, Clone, PartialEq)]
pub struct MentionQuery<'a> {
    // Byte offset of the '@' itself, so apply_mention can replace exactly
    // the right span regardless of whether it's quoted (and therefore may
    // contain spaces the naive whitespace-boundary logic would stop at).
    pub start: usize,
    pub query: &'a str,
}

// The @-mention query currently being typed, if any — e.g. "Buy milk @ja"
// -> query "ja". Once an opening quote follows the '@' (e.g. "Buy milk
// @\"Jane D") the query spans everything after that quote, spaces included,
// up until a closing quote appears (at which point tokenize()/parse() takes
// over and this returns None — the mention is either resolved or not, no
// longer "in progress"). This is what makes @"..." actually useful over
// bare @: without quotes, the query resets at the first space, so searching
// a multi-word name mid-typing ("Jane D...") never gets shown a dropdown.
//
// Deliberately end-of-string only (not cursor-position-aware): quick-capture
// mentions are typed live at the end of the field, so this avoids needing
// any DOM cursor-position access, which would need platform-specific
// (web vs desktop) handling.
pub fn trailing_mention_query(input: &str) -> Option<MentionQuery<'_>> {
    if let Some(at_pos) = input.rfind("@\"") {
        let before_ok = input[..at_pos].chars().next_back().map(|c| c.is_whitespace()).unwrap_or(true);
        let after_quote = at_pos + "@\"".len();
        let fragment = &input[after_quote..];
        if before_ok && !fragment.contains('"') {
            return Some(MentionQuery { start: at_pos, query: fragment });
        }
    }

    let last_word_start = input.char_indices().rev()
        .find(|&(_, c)| c.is_whitespace())
        .map(|(i, c)| i + c.len_utf8())
        .unwrap_or(0);
    let last_word = &input[last_word_start..];
    let query = last_word.strip_prefix('@')?;
    if query.starts_with('"') {
        // A closed quoted mention (handled above already returned, if still
        // open) — don't also treat it as an unquoted query.
        return None;
    }
    Some(MentionQuery { start: last_word_start, query })
}

// Replaces the mention span (from `start` through the end of the string)
// with the full, matched entity name plus a trailing space, so the mention
// becomes exact and unambiguous for tokenize()/parse() from then on.
pub fn apply_mention(current: &str, start: usize, entity_name: &str) -> String {
    format!("{}@{entity_name} ", &current[..start])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entities() -> Vec<EntityType> {
        vec![
            EntityType { id: "1".into(), name: "Jane".into(), entity_type_id: None, created_at: String::new(), drift: 2.0, metadata: None },
            EntityType { id: "2".into(), name: "Jane Doe".into(), entity_type_id: None, created_at: String::new(), drift: 2.0, metadata: None },
            EntityType { id: "0".into(), name: "Self".into(), entity_type_id: None, created_at: String::new(), drift: 2.0, metadata: None },
        ]
    }

    #[test]
    fn extracts_priority_project_and_tags() {
        let cap = parse("Buy flowers priority:H project:Home.Garden +errand", &entities());
        assert_eq!(cap.title, "Buy flowers");
        assert_eq!(cap.priority, Some("H".to_string()));
        assert_eq!(cap.project, Some("Home.Garden".to_string()));
        assert_eq!(cap.tags_add, vec!["errand".to_string()]);
    }

    #[test]
    fn pro_and_pri_are_shorthand_aliases() {
        let cap = parse("Buy flowers pri:H pro:Home.Garden", &entities());
        assert_eq!(cap.priority, Some("H".to_string()));
        assert_eq!(cap.project, Some("Home.Garden".to_string()));
    }

    #[test]
    fn wait_is_an_alias_for_scheduled() {
        let scheduled = parse("Call back scheduled:2026-08-01", &entities());
        let wait = parse("Call back wait:2026-08-01", &entities());
        assert_eq!(scheduled.scheduled_at, wait.scheduled_at);
        assert!(wait.scheduled_at.is_some());
    }

    #[test]
    fn semicolon_letter_semicolon_sets_moment_type() {
        assert_eq!(parse("Call mom ;t;", &entities()).moment_type_id, Some(1));
        assert_eq!(parse("Call mom ;p;", &entities()).moment_type_id, Some(2));
        assert_eq!(parse("Call mom ;n;", &entities()).moment_type_id, Some(3));
        assert_eq!(parse("Call mom", &entities()).moment_type_id, None);
    }

    #[test]
    fn depends_and_deps_capture_a_raw_title_to_resolve_later() {
        let cap = parse("Ship it depends:review", &entities());
        assert_eq!(cap.depends_on_title, Some("review".to_string()));
        assert_eq!(cap.title, "Ship it");

        let cap = parse("Ship it deps:\"final review\"", &entities());
        assert_eq!(cap.depends_on_title, Some("final review".to_string()));
        assert_eq!(cap.title, "Ship it");
    }

    #[test]
    fn matches_longest_entity_name() {
        let cap = parse("Call @Jane Doe about the wedding", &entities());
        assert_eq!(cap.entity_id, Some("2".to_string()));
        assert_eq!(cap.title, "Call about the wedding");
    }

    #[test]
    fn shorter_entity_name_matches_when_longer_not_present() {
        let cap = parse("Call @Jane tonight", &entities());
        assert_eq!(cap.entity_id, Some("1".to_string()));
        assert_eq!(cap.title, "Call tonight");
    }

    #[test]
    fn unmatched_mention_is_left_as_plain_title_text() {
        let cap = parse("email @nobody about it", &entities());
        assert_eq!(cap.entity_id, None);
        assert_eq!(cap.title, "email @nobody about it");
    }

    #[test]
    fn invalid_date_is_left_as_plain_text_not_activated() {
        let cap = parse("renew passport due:whenever", &entities());
        assert_eq!(cap.due_at, None);
        assert_eq!(cap.title, "renew passport due:whenever");
    }

    #[test]
    fn recognizes_relative_and_absolute_dates() {
        let cap = parse("pay rent due:tomorrow scheduled:2026-08-01 until:2026-08-15T09:30", &entities());
        assert!(cap.due_at.is_some());
        assert_eq!(cap.scheduled_at, Some("2026-08-01T00:00".to_string()));
        assert_eq!(cap.until_at, Some("2026-08-15T09:30".to_string()));
    }

    #[test]
    fn trailing_mention_query_detects_in_progress_typing() {
        assert_eq!(trailing_mention_query("Buy milk @ja").map(|m| m.query), Some("ja"));
        assert_eq!(trailing_mention_query("Buy milk @").map(|m| m.query), Some(""));
        assert_eq!(trailing_mention_query("Buy milk "), None);
        assert_eq!(trailing_mention_query("Buy milk@ja"), None);
    }

    #[test]
    fn trailing_mention_query_spans_spaces_inside_an_open_quote() {
        let q = trailing_mention_query("Call @\"Jane D").expect("in-progress quoted mention");
        assert_eq!(q.query, "Jane D");
        assert_eq!(q.start, "Call ".len());
    }

    #[test]
    fn trailing_mention_query_closes_once_the_quote_is_terminated() {
        assert_eq!(trailing_mention_query("Call @\"Jane Doe\" "), None);
        assert_eq!(trailing_mention_query("Call @\"Jane Doe\""), None);
    }

    #[test]
    fn apply_mention_replaces_from_the_given_start() {
        assert_eq!(apply_mention("Call @ja", "Call ".len(), "Jane Doe"), "Call @Jane Doe ");
        assert_eq!(apply_mention("@ja", 0, "Jane Doe"), "@Jane Doe ");
        assert_eq!(apply_mention("Call @\"Jane D", "Call ".len(), "Jane Doe"), "Call @Jane Doe ");
    }

    #[test]
    fn quoted_mention_matches_by_exact_name() {
        let cap = parse("Call @\"Jane Doe\" tonight", &entities());
        assert_eq!(cap.entity_id, Some("2".to_string()));
        assert_eq!(cap.title, "Call tonight");
    }

    #[test]
    fn unmatched_quoted_mention_stays_plain() {
        let cap = parse("Call @\"Nobody Here\" tonight", &entities());
        assert_eq!(cap.entity_id, None);
        assert_eq!(cap.title, "Call @\"Nobody Here\" tonight");
    }

    #[test]
    fn quoted_tag_can_contain_spaces() {
        let cap = parse("Reach out +\"corrupt journalist\" -\"old lead\"", &entities());
        assert_eq!(cap.tags_add, vec!["corrupt journalist".to_string()]);
        assert_eq!(cap.tags_remove, vec!["old lead".to_string()]);
        assert_eq!(cap.title, "Reach out");
    }

    #[test]
    fn unterminated_quote_is_left_plain_not_activated() {
        let cap = parse("Call @\"Jane and tell her", &entities());
        assert_eq!(cap.entity_id, None);
        assert_eq!(cap.title, "Call @\"Jane and tell her");
    }

    #[test]
    fn tokens_cover_whole_string_for_overlay_rendering() {
        let cases = [
            "Call @Jane priority:H  +errand",
            "Reach out +\"corrupt journalist\" @\"Jane Doe\"",
            "Call @\"Jane and tell her",
        ];
        for input in cases {
            let tokens = tokenize(input, &entities());
            let rebuilt: String = tokens.iter().map(|t| t.text.as_str()).collect();
            assert_eq!(rebuilt, input);
        }
    }
}
