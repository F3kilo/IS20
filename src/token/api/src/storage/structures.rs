use std::collections::{hash_map::Entry, HashMap};

use candid::Principal;
use canister_sdk::ic_kit::ic;
use ic_stable_structures::{btreemap, cell, log, memory_manager::MemoryId, Storable};

use super::Memory;

pub struct StableCell<T: Storable> {
    data: HashMap<Principal, cell::Cell<T, Memory>>,
    default_value: T,
    memory_id: MemoryId,
}

impl<T: Storable> StableCell<T> {
    pub fn new(memory_id: MemoryId, default_value: T) -> Self {
        Self {
            data: HashMap::default(),
            default_value,
            memory_id,
        }
    }

    pub fn get(&self) -> &T {
        let canister_id = ic::id();
        self.data
            .get(&canister_id)
            .map(|cell| cell.get())
            .unwrap_or(&self.default_value)
    }

    /// Updates value in stable memory.
    pub fn set(&mut self, value: T) {
        let canister_id = ic::id();
        match self.data.entry(canister_id) {
            Entry::Occupied(mut entry) => {
                entry
                    .get_mut()
                    .set(value)
                    .expect("failed to set value to stable cell");
            }
            Entry::Vacant(entry) => {
                let memory = super::get_memory_by_id(self.memory_id);
                entry.insert(cell::Cell::init(memory, value).expect("failed to init stable cell"));
            }
        };
    }
}

pub struct StableBTreeMap<K: Storable, V: Storable> {
    data: HashMap<Principal, btreemap::BTreeMap<Memory, K, V>>,
    memory_id: MemoryId,
    max_key_size: u32,
    max_value_size: u32,
}

impl<K: Storable, V: Storable> StableBTreeMap<K, V> {
    pub fn new(memory_id: MemoryId, max_key_size: u32, max_value_size: u32) -> Self {
        Self {
            data: HashMap::default(),
            memory_id,
            max_key_size,
            max_value_size,
        }
    }

    pub fn get(&self, key: &K) -> Option<V> {
        let canister_id = ic::id();
        let storage = self.data.get(&canister_id);
        storage.and_then(|m| m.get(key))
    }

    pub fn insert(&mut self, key: K, value: V) {
        let canister_id = ic::id();
        self.data
            .entry(canister_id)
            .or_insert_with(|| {
                let memory = super::get_memory_by_id(self.memory_id);
                btreemap::BTreeMap::init(memory, self.max_key_size, self.max_value_size)
            })
            .insert(key, value)
            .expect("failed to insert value to stable btree map");
    }

    pub fn remove(&mut self, key: &K) -> Option<V> {
        let canister_id = ic::id();
        self.data.get_mut(&canister_id)?.remove(key)
    }

    pub fn list(&self, start: usize, limit: usize) -> Vec<(K, V)> {
        let canister_id = ic::id();
        let storage = self.data.get(&canister_id);
        storage
            .iter()
            .flat_map(|s| s.iter())
            .skip(start)
            .take(limit)
            .collect()
    }
}

pub type StableLog = log::Log<Memory, Memory>;
