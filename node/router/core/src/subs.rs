// Copyright (C) 2019-2023 Aleo Systems Inc.
// This file is part of the snarkOS library.

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at:
// http://www.apache.org/licenses/LICENSE-2.0

// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use std::{
    borrow::Borrow,
    collections::{HashMap, HashSet},
    hash::Hash,
    sync::{
        atomic::{AtomicU32, Ordering},
        Arc,
    },
};

use axum::extract::ws::WebSocket;
use serde::Serialize;
use serde_json::json;
use tokio::sync::Mutex;
use tracing::warn;

type Stream = Arc<Mutex<WebSocket>>;
type StreamId = usize;
fn stream_id(stream: &Stream) -> StreamId {
    stream.as_ref() as *const _ as usize
}
pub struct SubscribeContext<S> {
    pub stream_id: StreamId,
    pub stream_seq: AtomicU32,
    pub settings: S,
}

pub struct SubscriptionManager<S, Key = ()> {
    pub stream_code: u32,
    pub subscribes: HashMap<StreamId, SubscribeContext<S>>,
    pub mappings: HashMap<Key, HashSet<StreamId>>,
    pub streams: HashMap<StreamId, Stream>,
}

impl<S, Key: Hash + Eq> SubscriptionManager<S, Key> {
    pub fn new(stream_code: u32) -> Self {
        Self { stream_code, subscribes: Default::default(), mappings: Default::default(), streams: Default::default() }
    }

    pub fn subscribe(
        &mut self,
        stream: &Stream,
        setting: S,
        modify: impl FnOnce(&mut SubscribeContext<S>),
    ) -> StreamId {
        self.subscribe_with(stream, vec![], || setting, modify)
    }

    pub fn subscribe_by_id(&mut self, id: StreamId, setting: S, modify: impl FnOnce(&mut SubscribeContext<S>)) {
        self.subscribe_with_id(id, vec![], || setting, modify)
    }

    pub fn subscribe_with(
        &mut self,
        stream: &Stream,
        keys: Vec<Key>,
        new: impl FnOnce() -> S,
        modify: impl FnOnce(&mut SubscribeContext<S>),
    ) -> StreamId {
        let id = stream_id(stream);
        self.subscribe_with_id(id, keys, new, modify);
        self.streams.insert(id, stream.clone());
        id
    }

    pub fn subscribe_with_id(
        &mut self,
        id: StreamId,
        keys: Vec<Key>,
        new: impl FnOnce() -> S,
        modify: impl FnOnce(&mut SubscribeContext<S>),
    ) {
        self.subscribes.entry(id).and_modify(modify).or_insert_with(|| SubscribeContext {
            stream_id: id,
            stream_seq: AtomicU32::new(0),
            settings: new(),
        });
        for key in keys {
            self.mappings.entry(key).or_default().insert(id);
        }
    }

    pub fn unsubscribe(&mut self, stream: &Stream) {
        let id = stream_id(stream);
        self.unsubscribe_by_id(id)
    }

    pub fn unsubscribe_by_id(&mut self, id: StreamId) {
        self.subscribes.remove(&id);
        for pair in self.mappings.values_mut() {
            pair.remove(&id);
        }
        self.streams.remove(&id);
    }

    pub fn remove_key(&mut self, key: &Key, mut remove_key: impl FnMut(&mut SubscribeContext<S>) -> bool) {
        let Some(set) = self.mappings.remove(key) else {
            return;
        };

        for id in set {
            let Some(ctx) = self.subscribes.get_mut(&id) else {
                continue;
            };
            let remove = remove_key(ctx);
            if remove {
                self.subscribes.remove(&id);
                self.streams.remove(&id);
            }
        }
    }

    pub fn remove_keys(&mut self, keys: &[Key], mut remove: impl FnMut(&mut SubscribeContext<S>, &[&Key]) -> bool) {
        let mut remove_by_keys = HashMap::new();
        for key in keys {
            if let Some(set) = self.mappings.remove(key) {
                for id in set {
                    remove_by_keys.entry(id).or_insert_with(Vec::new).push(key);
                }
            }
        }
        for (id, keys) in remove_by_keys {
            let Some(ctx) = self.subscribes.get_mut(&id) else {
                continue;
            };
            let remove = remove(ctx, &keys);
            if remove {
                self.subscribes.remove(&id);
                self.streams.remove(&id);
            }
        }
    }

    pub fn unsubscribe_with(&mut self, stream: &Stream, remove: impl Fn(&mut SubscribeContext<S>) -> (bool, Vec<Key>)) {
        let id = stream_id(stream);
        let Some(sub) = self.subscribes.get_mut(&id) else {
            return;
        };

        let (remove1, keys) = remove(&mut *sub);

        for key in keys {
            let remove = self
                .mappings
                .get_mut(&key)
                .map(|set| {
                    set.remove(&id);
                    set.is_empty()
                })
                .unwrap_or_default();
            if remove {
                self.mappings.remove(&key);
            }
        }
        if remove1 {
            self.subscribes.remove(&id);
            self.streams.remove(&id);
        }
    }

    pub async fn publish_to(&mut self, stream: &Stream, msg: &impl Serialize) {
        let find = self.subscribes.get(&stream_id(stream));
        let Some(sub) = find else { return };

        let data = serde_json::to_value(msg).unwrap();
        let mut dead_connection = None;
        sub.stream_seq.fetch_add(1, Ordering::SeqCst);
        if let Err(err) = stream.lock().await.send(data.to_string().into()).await {
            warn!("failed to send message to stream: {}", err);
            dead_connection = Some(stream_id(stream));
        }

        if let Some(conn_id) = dead_connection {
            self.unsubscribe_by_id(conn_id)
        }
    }

    pub async fn publish_to_id(&mut self, id: StreamId, msg: &impl Serialize) {
        let stream = self.streams.get(&id).cloned();
        if let Some(stream) = stream {
            self.publish_to(&stream, msg).await;
        }
    }

    pub async fn publish_to_key<Q>(&mut self, key: &Q, msg: &impl Serialize)
    where
        Key: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        let Some(conn_ids) = self.mappings.get(key).cloned() else {
            return;
        };

        for conn_id in conn_ids {
            let stream = self.subscribes.get(&conn_id).map(|x| x.stream_id);
            if let Some(stream) = stream {
                self.publish_to_id(stream, msg).await;
            }
        }
    }

    pub async fn publish_to_keys<Q>(&mut self, keys: &[&Q], msg: &impl Serialize)
    where
        Key: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        let mut published = HashSet::new();
        for key in keys {
            let conn_ids = self.mappings.get(key).cloned();
            if let Some(conn_ids) = conn_ids {
                for conn_id in conn_ids.iter() {
                    // if newly inserted
                    if published.insert(*conn_id) {
                        self.publish_to_id(*conn_id, msg).await;
                    }
                }
            }
        }
    }

    pub async fn publish_with_filter<M: Serialize>(&mut self, filter: impl Fn(&SubscribeContext<S>) -> Option<M>) {
        let mut dead_connections = vec![];

        for sub in self.subscribes.values() {
            let Some(data) = filter(sub) else {
                continue;
            };
            let data = serde_json::to_value(&data).unwrap();
            let msg = json!({
                "stream_seq": sub.stream_seq.fetch_add(1, Ordering::SeqCst),
                "stream_code": self.stream_code,
                "data": data,
            })
            .to_string();
            let Some(stream) = self.streams.get(&sub.stream_id).cloned() else {
                continue;
            };
            let send = stream.lock().await.send(msg.into()).await;
            if let Err(err) = send {
                warn!("failed to send message to stream: {}", err);
                dead_connections.push(sub.stream_id);
            }
        }
        for conn_id in dead_connections {
            self.unsubscribe_by_id(conn_id)
        }
    }

    pub async fn publish_to_all(&mut self, msg: &impl Serialize) {
        self.publish_with_filter(|_| Some(msg)).await
    }
}

#[cfg(test)]
mod tests {
    pub(super) use super::*;
    #[tokio::test]
    async fn test_subscribe() {
        let mut manager: SubscriptionManager<(), ()> = SubscriptionManager::new(0);

        let id = 1;
        manager.subscribe_by_id(id, (), |_| {});
        assert_eq!(manager.subscribes.len(), 1);
        assert_eq!(manager.streams.len(), 0);
        assert_eq!(manager.mappings.len(), 0);
        manager.publish_to_all(&()).await;
        manager.publish_to_key(&(), &()).await;
        manager.publish_to_keys(&[], &()).await;
        manager.unsubscribe_by_id(id);
        assert_eq!(manager.subscribes.len(), 0);
        assert_eq!(manager.streams.len(), 0);
        assert_eq!(manager.mappings.len(), 0);
    }
}
