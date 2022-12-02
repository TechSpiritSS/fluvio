use std::fs::File;
use serde::{Deserialize, Serialize};

use fluvio::{Compression, config::ConfigFile};
use super::{BenchmarkConfig, BenchmarkConfigBuilder, CrossIterate, Millis, Seconds};

/// Key used by AllShareSameKey
pub const SHARED_KEY: &str = "SHARED_KEY";

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SharedConfig {
    pub matrix_name: String,
    pub num_samples: usize,
    pub millis_between_samples: Millis,
    pub worker_timeout_seconds: Seconds,
}

/// Corresponds to https://docs.rs/fluvio/latest/fluvio/struct.TopicProducerConfigBuilder.html
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct FluvioProducerConfig {
    pub batch_size: Vec<u64>,
    pub queue_size: Vec<u64>,
    pub linger_millis: Vec<Millis>,
    pub server_timeout_millis: Vec<Millis>,
    pub compression: Vec<Compression>,
    // TODO
    // pub producer_isolation:...,
    // TODO
    // pub producer_delivery_semantic,
}

/// Corresponds to https://docs.rs/fluvio/latest/fluvio/consumer/struct.ConsumerConfigBuilder.html
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct FluvioConsumerConfig {
    pub max_bytes: Vec<u64>,
    // TODO
    // pub consumer_isolation:...,
}

/// Corresponds to https://docs.rs/fluvio/latest/fluvio/metadata/topic/struct.TopicSpec.html
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct FluvioTopicConfig {
    pub num_partitions: Vec<u64>,
    // TODO
    // pub use_smart_module: Vec<bool>,
    // TODO
    // IgnoreRack
    // TODO
    // pub num_replicas: Vec<u64>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct BenchmarkLoadConfig {
    pub num_records_per_producer_worker_per_batch: Vec<u64>,
    pub record_key_allocation_strategy: Vec<RecordKeyAllocationStrategy>,
    pub num_concurrent_producer_workers: Vec<u64>,
    /// Total number of concurrent consumers equals num_concurrent_consumers_per_partition * num_partitions
    pub num_concurrent_consumers_per_partition: Vec<u64>,
    pub record_size: Vec<u64>,
}

/// A BenchmarkMatrix contains shared config for all runs and dimensions that hold values that will change across runs.
/// Iterating over a BenchmarkMatrix produces a BenchmarkConfig for every possible combination of values in the matrix.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct BenchmarkMatrix {
    pub shared_config: SharedConfig,
    pub producer_config: FluvioProducerConfig,
    pub consumer_config: FluvioConsumerConfig,
    pub topic_config: FluvioTopicConfig,
    pub load_config: BenchmarkLoadConfig,
}

impl IntoIterator for BenchmarkMatrix {
    type Item = BenchmarkConfig;

    type IntoIter = <Vec<BenchmarkConfig> as IntoIterator>::IntoIter;

    fn into_iter(self) -> Self::IntoIter {
        self.generate_configs().into_iter()
    }
}

impl BenchmarkMatrix {
    // Impl note: This does allocate for all of the benchmark configs at once, however it made for simpler code
    // and as there is a very low practical limit for the number of benchmarks that can be run in a reasonable time period, its not an issue that it allocates.

    fn generate_configs(&self) -> Vec<BenchmarkConfig> {
        let profile_name = ConfigFile::load_default_or_new()
            .map(|config_file| {
                config_file
                    .config()
                    .current_profile_name()
                    .map(|s| s.to_string())
            })
            .ok()
            .flatten()
            .unwrap_or_else(|| "Unknown".to_string());

        let builder = vec![BenchmarkConfigBuilder::new(
            &self.shared_config,
            profile_name,
        )];
        builder
            .cross_iterate(
                &self.load_config.num_records_per_producer_worker_per_batch,
                |v, b| {
                    b.num_records_per_producer_worker_per_batch(v);
                },
            )
            .cross_iterate(&self.producer_config.batch_size, |v, b| {
                b.producer_batch_size(v);
            })
            .cross_iterate(&self.producer_config.queue_size, |v, b| {
                b.producer_queue_size(v);
            })
            .cross_iterate(&self.producer_config.linger_millis, |v, b| {
                b.producer_linger(v.into());
            })
            .cross_iterate(&self.producer_config.server_timeout_millis, |v, b| {
                b.producer_server_timeout(v.into());
            })
            .cross_iterate(&self.producer_config.compression, |v, b| {
                b.producer_compression(v);
            })
            .cross_iterate(&self.consumer_config.max_bytes, |v, b| {
                b.consumer_max_bytes(v);
            })
            .cross_iterate(&self.load_config.num_concurrent_producer_workers, |v, b| {
                b.num_concurrent_producer_workers(v);
            })
            .cross_iterate(
                &self.load_config.num_concurrent_consumers_per_partition,
                |v, b| {
                    b.num_concurrent_consumers_per_partition(v);
                },
            )
            .cross_iterate(&self.topic_config.num_partitions, |v, b| {
                b.num_partitions(v);
            })
            .cross_iterate(&self.load_config.record_size, |v, b| {
                b.record_size(v);
            })
            .cross_iterate(&self.load_config.record_key_allocation_strategy, |v, b| {
                b.record_key_allocation_strategy(v);
            })
            .build()
    }
}

#[derive(Debug, Serialize, Deserialize, Copy, Clone, Hash, PartialEq, Eq)]
pub enum RecordKeyAllocationStrategy {
    /// RecordKey::NULL
    NoKey,
    /// All producer workers will use the same key
    AllShareSameKey,

    /// Each producer will use the same key for each of their records
    ProducerWorkerUniqueKey,

    /// Each producer will round robin from 0..N for each record produced
    RoundRobinKey(u64),

    RandomKey,
}

pub fn get_config_from_file(path: &str) -> Vec<BenchmarkMatrix> {
    let file = File::open(path).unwrap();
    vec![serde_yaml::from_reader::<_, BenchmarkMatrix>(file).unwrap()]
}