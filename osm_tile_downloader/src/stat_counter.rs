use serde::Deserialize;
use serde::Serialize;


use crate::config::get_current_timestamp;


lazy_static::lazy_static! {
    pub static ref DB_STAT_COUNTER:
        typed_sled::Tree::<StatCounterKey, StatCounterVal>
         = typed_sled::Tree::<StatCounterKey, StatCounterVal>::open(&crate::config::SLED_DB, "stat_counter_3");
}


#[derive(
    Serialize, Deserialize, Clone, Debug, PartialEq, Hash, Eq, PartialOrd, Ord,
)]
pub struct StatCounterKey {
    pub stat_type: String,
    pub item_a: String,
    pub item_b: String,
}
use std::collections::HashMap;
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct StatCounterVal {
    event_count: HashMap<String, u64>,
    edit_at: f64,
}

const STAT_COUNTER_ENTRY_TTL: f64 = 7300.0;

impl StatCounterVal {
    fn increment(&mut self, event: &str) {
        self.event_count.insert(
            event.to_owned(),
            self.event_count.get(event).unwrap_or(&0) + 1,
        );
        self.edit_at = get_current_timestamp();
    }
}

pub fn stat_counter_increment(
    stat_type: &str,
    stat_event: &str,
    stat_item_a: &str,
    stat_item_b: &str,
) -> anyhow::Result<()> {
    let hash_key = StatCounterKey {
        stat_type: stat_type.to_owned(),
        item_a: stat_item_a.to_owned(),
        item_b: stat_item_b.to_owned(),
    };

    DB_STAT_COUNTER.update_and_fetch(&hash_key.to_owned(), |v| match v {
        Some(mut stat_counter) => {
            stat_counter.increment(stat_event);
            Some(stat_counter)
        }
        None => {
            let mut stat_counter = StatCounterVal {
                event_count: HashMap::new(),
                edit_at: get_current_timestamp(),
            };
            stat_counter.increment(stat_event);
            Some(stat_counter)
        }
    })?;
    Ok(())
}

pub fn stat_counter_get_all() -> Vec<(StatCounterKey, String, u64)> {
    let mut _vec = vec![];
    let mut _keys_to_delete = vec![];

    DB_STAT_COUNTER.iter().for_each(|x| {
        if let Ok((hash_key, v)) = x {
            if v.edit_at + STAT_COUNTER_ENTRY_TTL < get_current_timestamp() {
                _keys_to_delete.push(hash_key.clone());
                return;
            }
            for (event, counter) in v.event_count.iter() {
                _vec.push((hash_key.clone(), event.clone(), *counter));
            }
        }
    });
    _vec.sort();
    _vec
}

pub fn stat_count_events_for_items(
    items: &Vec<&str>,
) -> HashMap<String, HashMap<String, u64>> {
    let mut _map = HashMap::<String, HashMap<String, u64>>::new();
    for item in items.iter() {
        _map.insert(item.to_string(), HashMap::<String, u64>::new());
    }

    for (key, event, count) in stat_counter_get_all() {
        for item in items {
            if key.item_a.eq(item) || key.item_b.eq(item) {
                let mut _sub_map = _map.get_mut(*item).unwrap();
                let old_count = _sub_map.get(&event.clone()).unwrap_or(&0);
                _sub_map.insert(event.clone(), count + old_count);
            }
        }
    }
    _map
}