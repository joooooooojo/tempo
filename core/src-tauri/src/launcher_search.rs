use pinyin::ToPinyin;
use std::collections::HashSet;
use unicode_normalization::UnicodeNormalization;

#[derive(Debug, Clone, Default)]
pub struct SearchTextIndex {
    fields: Vec<SearchField>,
}

#[derive(Debug, Clone)]
struct SearchField {
    kind: SearchFieldKind,
    value: String,
    compact: String,
    words: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum SearchFieldKind {
    Name,
    Keyword,
    Pinyin,
    PinyinAbbr,
    Acronym,
}

#[derive(Clone, Copy)]
struct FieldScores {
    exact: u32,
    prefix: u32,
    words: u32,
    contains: u32,
    typo: u32,
}

impl SearchTextIndex {
    pub fn new(name: &str, keywords: &[String]) -> Self {
        let mut fields = Vec::new();
        let mut seen = HashSet::new();

        add_term(&mut fields, &mut seen, SearchFieldKind::Name, name);
        for keyword in keywords {
            add_term(&mut fields, &mut seen, SearchFieldKind::Keyword, keyword);
        }

        Self { fields }
    }

    pub fn score(&self, raw_query: &str) -> Option<u32> {
        let query = normalize(raw_query);
        let compact_query = compact(&query);
        if query.is_empty() || compact_query.is_empty() {
            return None;
        }

        self.fields
            .iter()
            .map(|field| score_field(field, &query, &compact_query))
            .max()
            .filter(|score| *score > 0)
    }
}

fn add_term(
    fields: &mut Vec<SearchField>,
    seen: &mut HashSet<(SearchFieldKind, String)>,
    kind: SearchFieldKind,
    raw_value: &str,
) {
    add_field(fields, seen, kind, raw_value);

    for alias in extract_english_aliases(raw_value) {
        add_field(fields, seen, SearchFieldKind::Acronym, &alias);
    }

    if let Some((full, initials)) = pinyin_forms(raw_value) {
        add_field(fields, seen, SearchFieldKind::Pinyin, &full);
        add_field(fields, seen, SearchFieldKind::PinyinAbbr, &initials);
    }
}

fn add_field(
    fields: &mut Vec<SearchField>,
    seen: &mut HashSet<(SearchFieldKind, String)>,
    kind: SearchFieldKind,
    raw_value: &str,
) {
    let value = normalize(raw_value);
    let compact_value = compact(&value);
    if value.is_empty() || compact_value.is_empty() || !seen.insert((kind, compact_value.clone())) {
        return;
    }

    fields.push(SearchField {
        kind,
        words: split_words(&value),
        value,
        compact: compact_value,
    });
}

fn score_field(field: &SearchField, query: &str, compact_query: &str) -> u32 {
    let scores = field_scores(field.kind);
    let position = char_position(&field.compact, compact_query);
    let coverage_bonus = ((compact_query.chars().count() * 500)
        / field.compact.chars().count().max(1))
    .min(500) as u32;
    let position_bonus = position
        .map(|position| 300u32.saturating_sub((position as u32).saturating_mul(10)))
        .unwrap_or(0);

    if field.value == query || field.compact == compact_query {
        return scores.exact + coverage_bonus;
    }
    if field.value.starts_with(query) || field.compact.starts_with(compact_query) {
        return scores.prefix + coverage_bonus;
    }

    let query_words = split_words(query);
    if scores.words > 0
        && !query_words.is_empty()
        && query_words
            .iter()
            .all(|query_word| field.words.iter().any(|word| word.starts_with(query_word)))
    {
        return scores.words + coverage_bonus;
    }

    if position.is_some() {
        return scores.contains + coverage_bonus + position_bonus;
    }

    if scores.typo > 0 {
        if let Some(distance) = typo_distance(compact_query, field) {
            return scores.typo - (distance as u32 * 2_000) + coverage_bonus;
        }
    }

    0
}

fn field_scores(kind: SearchFieldKind) -> FieldScores {
    match kind {
        SearchFieldKind::Name => FieldScores {
            exact: 100_000,
            prefix: 80_000,
            words: 72_000,
            contains: 60_000,
            typo: 30_000,
        },
        SearchFieldKind::Keyword => FieldScores {
            exact: 90_000,
            prefix: 70_000,
            words: 64_000,
            contains: 50_000,
            typo: 26_000,
        },
        SearchFieldKind::Acronym => FieldScores {
            exact: 87_000,
            prefix: 77_000,
            words: 0,
            contains: 57_000,
            typo: 0,
        },
        SearchFieldKind::PinyinAbbr => FieldScores {
            exact: 86_000,
            prefix: 76_000,
            words: 0,
            contains: 56_000,
            typo: 27_000,
        },
        SearchFieldKind::Pinyin => FieldScores {
            exact: 85_000,
            prefix: 74_000,
            words: 0,
            contains: 54_000,
            typo: 27_000,
        },
    }
}

fn typo_distance(query: &str, field: &SearchField) -> Option<usize> {
    if query.len() < 4
        || field.compact.len() > 64
        || !query.is_ascii()
        || !field.compact.is_ascii()
        || (field.kind == SearchFieldKind::Keyword
            && (field.value.contains('\\')
                || field.value.contains('/')
                || field.value.contains(':')))
    {
        return None;
    }

    let maximum = if query.len() >= 8 { 2 } else { 1 };
    let mut candidates = field.words.iter().map(String::as_str).collect::<Vec<_>>();
    if candidates.len() > 1 {
        candidates.push(field.compact.as_str());
    }

    candidates
        .into_iter()
        .filter(|candidate| candidate.len().abs_diff(query.len()) <= maximum)
        .filter_map(|candidate| {
            let distance = bounded_damerau_levenshtein(query, candidate, maximum);
            (distance > 0 && distance <= maximum).then_some(distance)
        })
        .min()
}

fn bounded_damerau_levenshtein(left: &str, right: &str, maximum: usize) -> usize {
    if left == right {
        return 0;
    }
    let left = left.chars().collect::<Vec<_>>();
    let right = right.chars().collect::<Vec<_>>();
    if left.len().abs_diff(right.len()) > maximum {
        return maximum + 1;
    }

    let mut previous_previous = (0..=right.len()).collect::<Vec<_>>();
    let mut previous = previous_previous.clone();

    for left_index in 1..=left.len() {
        let mut current = vec![left_index; right.len() + 1];
        let mut row_minimum = left_index;

        for right_index in 1..=right.len() {
            let substitution = usize::from(left[left_index - 1] != right[right_index - 1]);
            let mut distance = (current[right_index - 1] + 1)
                .min(previous[right_index] + 1)
                .min(previous[right_index - 1] + substitution);

            if left_index > 1
                && right_index > 1
                && left[left_index - 1] == right[right_index - 2]
                && left[left_index - 2] == right[right_index - 1]
            {
                distance = distance.min(previous_previous[right_index - 2] + 1);
            }

            current[right_index] = distance;
            row_minimum = row_minimum.min(distance);
        }

        if row_minimum > maximum {
            return maximum + 1;
        }
        previous_previous = previous;
        previous = current;
    }

    previous[right.len()]
}

fn pinyin_forms(value: &str) -> Option<(String, String)> {
    let mut full = String::new();
    let mut initials = String::new();
    let mut converted = false;

    for character in value.chars() {
        if let Some(pinyin) = character.to_pinyin() {
            full.push_str(pinyin.plain());
            initials.push_str(pinyin.first_letter());
            converted = true;
        } else if character.is_alphanumeric() {
            full.extend(character.to_lowercase());
            initials.extend(character.to_lowercase());
        }
    }

    converted.then_some((full, initials))
}

fn extract_english_aliases(value: &str) -> Vec<String> {
    let words = value
        .split(|character: char| !character.is_ascii_alphanumeric())
        .filter(|word| !word.is_empty())
        .collect::<Vec<_>>();
    if words.len() > 1 {
        let acronym = words
            .iter()
            .filter_map(|word| word.chars().next())
            .flat_map(char::to_lowercase)
            .collect::<String>();
        let mut aliases = vec![acronym];

        // Common product queries retain the final word while abbreviating its qualifiers:
        // "Visual Studio Code" -> "vscode" in addition to the regular "vsc".
        if words.len() > 2 {
            let mut qualifier_initials = words[..words.len() - 1]
                .iter()
                .filter_map(|word| word.chars().next())
                .flat_map(char::to_lowercase)
                .collect::<String>();
            qualifier_initials.extend(words[words.len() - 1].chars().flat_map(char::to_lowercase));
            aliases.push(qualifier_initials);
        }

        return aliases;
    }

    let capitals = value
        .chars()
        .filter(|character| character.is_ascii_uppercase())
        .flat_map(char::to_lowercase)
        .collect::<String>();
    if capitals.len() > 1 {
        vec![capitals]
    } else {
        Vec::new()
    }
}

fn normalize(value: &str) -> String {
    value
        .nfkc()
        .flat_map(char::to_lowercase)
        .collect::<String>()
        .trim()
        .to_string()
}

fn compact(value: &str) -> String {
    value
        .chars()
        .filter(|character| character.is_alphanumeric())
        .collect()
}

fn split_words(value: &str) -> Vec<String> {
    value
        .split(|character: char| !character.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .map(str::to_string)
        .collect()
}

fn char_position(haystack: &str, needle: &str) -> Option<usize> {
    haystack
        .find(needle)
        .map(|byte_index| haystack[..byte_index].chars().count())
}

#[cfg(test)]
mod tests {
    use super::SearchTextIndex;

    #[test]
    fn indexes_full_pinyin_and_initials() {
        let index = SearchTextIndex::new("企业微信", &[]);
        assert!(index.score("qiyeweixin").is_some());
        assert!(index.score("qywx").is_some());
        assert!(index.score("wx").is_some());
    }

    #[test]
    fn indexes_english_acronyms_and_compact_names() {
        let index = SearchTextIndex::new("Visual Studio Code", &[]);
        assert!(index.score("vsc").is_some());
        assert!(index.score("vscode").is_some());
        assert!(index.score("visualstudiocode").is_some());
    }

    #[test]
    fn exact_and_prefix_matches_outrank_contains_and_typos() {
        let index = SearchTextIndex::new("Chrome Browser", &[]);
        let exact = index.score("chrome browser").unwrap();
        let prefix = index.score("chrome").unwrap();
        let contains = index.score("browser").unwrap();
        let typo = index.score("chorme").unwrap();
        assert!(exact > prefix);
        assert!(prefix > contains);
        assert!(contains > typo);
    }

    #[test]
    fn keywords_are_searchable_but_do_not_beat_exact_names() {
        let keyword = SearchTextIndex::new("Editor", &["code".into()]);
        let name = SearchTextIndex::new("Code", &[]);
        assert!(name.score("code").unwrap() > keyword.score("code").unwrap());
    }
}
