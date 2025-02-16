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
    collections::{BTreeMap, HashMap},
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};

use clickhouse_rs::{Block as DbBlock, ClientHandle};
use parking_lot::Mutex;
#[cfg(not(feature = "serial"))]
use rayon::prelude::*;
use snarkvm::{
    ledger::narwhal::TransmissionID,
    prelude::{cfg_iter, Block, Network},
};
// Re-export the snarkVM metrics.
pub use snarkvm::metrics::*;
use time::OffsetDateTime;

use model::OpenMetricsGaugeLine;
// Expose the names at the crate level for easy access.
pub use names::*;

pub mod model;
mod names;

/// Initializes the metrics and returns a handle to the task running the metrics exporter.
pub fn initialize_metrics() {
    // Build the Prometheus exporter.
    metrics_exporter_prometheus::PrometheusBuilder::new().install().expect("can't build the prometheus exporter");

    // Register the snarkVM metrics.
    register_metrics();

    // let guage = _register_gauge();
    // metrics.push(guage);
    // Register the metrics so they exist on init.
    for name in GAUGE_NAMES {
        register_gauge(name);
        // init_clickhouse(client, name);
    }
    for name in COUNTER_NAMES {
        register_counter(name);
    }
    for name in HISTOGRAM_NAMES {
        register_histogram(name);
    }
}

fn _register_gauge(gauge_name: &str, label_keys: Vec<String>, values: Vec<String>, value: f64) -> OpenMetricsGaugeLine {
    let mut labels: BTreeMap<String, String> = BTreeMap::new();
    for (index, key) in label_keys.iter().enumerate() {
        labels.insert(key.to_string(), values[index].clone());
    }
    let name = gauge_name;
    OpenMetricsGaugeLine::new(name, labels, value)
}

pub fn update_block_metrics<N: Network>(block: &Block<N>) {
    use snarkvm::ledger::ConfirmedTransaction;

    let accepted_deploy = AtomicUsize::new(0);
    let accepted_execute = AtomicUsize::new(0);
    let rejected_deploy = AtomicUsize::new(0);
    let rejected_execute = AtomicUsize::new(0);

    // Add transaction to atomic counter based on enum type match.
    cfg_iter!(block.transactions()).for_each(|tx| match tx {
        ConfirmedTransaction::AcceptedDeploy(_, _, _) => {
            accepted_deploy.fetch_add(1, Ordering::Relaxed);
        }
        ConfirmedTransaction::AcceptedExecute(_, _, _) => {
            accepted_execute.fetch_add(1, Ordering::Relaxed);
        }
        ConfirmedTransaction::RejectedDeploy(_, _, _, _) => {
            rejected_deploy.fetch_add(1, Ordering::Relaxed);
        }
        ConfirmedTransaction::RejectedExecute(_, _, _, _) => {
            rejected_execute.fetch_add(1, Ordering::Relaxed);
        }
    });

    increment_gauge(blocks::ACCEPTED_DEPLOY, accepted_deploy.load(Ordering::Relaxed) as f64);
    increment_gauge(blocks::ACCEPTED_EXECUTE, accepted_execute.load(Ordering::Relaxed) as f64);
    increment_gauge(blocks::REJECTED_DEPLOY, rejected_deploy.load(Ordering::Relaxed) as f64);
    increment_gauge(blocks::REJECTED_EXECUTE, rejected_execute.load(Ordering::Relaxed) as f64);

    // Update aborted transactions and solutions.
    increment_gauge(blocks::ABORTED_TRANSACTIONS, block.aborted_transaction_ids().len() as f64);
    increment_gauge(blocks::ABORTED_SOLUTIONS, block.aborted_solution_ids().len() as f64);
}

pub async fn init_clickhouse(mut client: ClientHandle, gauge_name: &str) {
    let create_table_query = format!(
        "CREATE TABLE IF NOT EXISTS {} ( \
            id UInt32, \
            name String \
        ) ENGINE = MergeTree \
        ORDER BY id",
        gauge_name
    );
    client.execute(create_table_query.as_str()).await.unwrap();
}
pub async fn add_clickhouse_record(mut client: ClientHandle, name: &str, label_keys: Vec<String>, values: Vec<String>) {
    let block = DbBlock::new().column("label_keys", label_keys).column("values", values);
    client.insert(name, block).await.unwrap();
}

pub fn add_transmission_latency_metric<N: Network>(
    transmissions_queue_timestamps: &Arc<Mutex<HashMap<TransmissionID<N>, i64>>>,
    block: &Block<N>,
) {
    const AGE_THRESHOLD_SECONDS: i32 = 30 * 60; // 30 minutes set as stale transmission threshold

    // Retrieve the solution IDs.
    let solution_ids: std::collections::HashSet<_> =
        block.solutions().solution_ids().chain(block.aborted_solution_ids()).collect();

    // Retrieve the transaction IDs.
    let transaction_ids: std::collections::HashSet<_> =
        block.transaction_ids().chain(block.aborted_transaction_ids()).collect();

    let mut transmission_queue_timestamps = transmissions_queue_timestamps.lock();
    let ts_now = OffsetDateTime::now_utc().unix_timestamp();

    // Determine which keys to remove.
    let keys_to_remove = cfg_iter!(transmission_queue_timestamps)
        .flat_map(|(key, timestamp)| {
            let elapsed_time = std::time::Duration::from_secs((ts_now - *timestamp) as u64);

            if elapsed_time.as_secs() > AGE_THRESHOLD_SECONDS as u64 {
                // This entry is stale-- remove it from transmission queue and record it as a stale transmission.
                increment_counter(consensus::STALE_UNCONFIRMED_TRANSMISSIONS);
                Some(*key)
            } else {
                let transmission_type = match key {
                    TransmissionID::Solution(solution_id) if solution_ids.contains(solution_id) => Some("solution"),
                    TransmissionID::Transaction(transaction_id) if transaction_ids.contains(transaction_id) => {
                        Some("transaction")
                    }
                    _ => None,
                };

                if let Some(transmission_type_string) = transmission_type {
                    histogram_label(
                        consensus::TRANSMISSION_LATENCY,
                        "transmission_type",
                        transmission_type_string.to_owned(),
                        elapsed_time.as_secs_f64(),
                    );
                    Some(*key)
                } else {
                    None
                }
            }
        })
        .collect::<Vec<_>>();

    // Remove keys of stale or seen transmissions.
    for key in keys_to_remove {
        transmission_queue_timestamps.remove(&key);
    }
}
