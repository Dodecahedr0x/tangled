use crate::replica::Replica;
use crate::types::Post;

pub fn search(replica: &Replica, query: &str) -> Vec<Post> {
    let q = query.to_lowercase();
    if q.is_empty() {
        return Vec::new();
    }
    replica
        .all_posts()
        .into_iter()
        .filter(|p| !p.deleted)
        .filter(|p| {
            p.text.to_lowercase().contains(&q)
                || p.handle
                    .as_ref()
                    .is_some_and(|h| h.to_lowercase().contains(&q))
                || p.author.to_lowercase().contains(&q)
        })
        .collect()
}

pub fn mentions(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for token in text.split_whitespace() {
        let t = token.trim_matches(|c: char| {
            matches!(
                c,
                ',' | '.' | ';' | ':' | '!' | '?' | ')' | '(' | '"' | '\''
            )
        });
        if let Some(rest) = t.strip_prefix('@') {
            if !rest.is_empty() && !out.iter().any(|h| h == rest) {
                out.push(rest.to_string());
            }
        }
    }
    out
}
