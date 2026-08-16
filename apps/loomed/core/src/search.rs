use crate::replica::Replica;
use crate::types::Message;

pub fn search(replica: &Replica, query: &str) -> Vec<Message> {
    let q = query.to_lowercase();
    replica
        .conversations()
        .into_iter()
        .flat_map(|c| replica.messages(&c.circle))
        .filter(|m| {
            m.text
                .as_ref()
                .is_some_and(|t| t.to_lowercase().contains(&q))
        })
        .collect()
}
