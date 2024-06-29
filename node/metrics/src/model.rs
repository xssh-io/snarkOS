use std::collections::BTreeMap;

#[allow(async_fn_in_trait)]
pub trait OpenMetricsExport {
    async fn to_string(&self) -> String;
}
impl<T: OpenMetricsExport> OpenMetricsExport for tokio::sync::RwLock<T> {
    async fn to_string(&self) -> String {
        self.read().await.to_string().await
    }
}
#[derive(Debug, PartialEq)]
pub struct OpenMetricsGaugeLine {
    pub name: String,
    pub labels: BTreeMap<String, String>,
    pub counter: f64,
    pub counter_type: String,
    pub metrics_type: String,
}

impl OpenMetricsGaugeLine {
    pub fn new(
        name: &str,
        labels: BTreeMap<String, String>,
        counter: f64,
        counter_type: &str,
        metrics_type: &str,
    ) -> Self {
        OpenMetricsGaugeLine {
            name: name.to_string(),
            labels,
            counter,
            counter_type: counter_type.to_string(),
            metrics_type: metrics_type.to_string(),
        }
    }
}
impl OpenMetricsExport for OpenMetricsGaugeLine {
    async fn to_string(&self) -> String {
        if self.labels.is_empty() {
            return format!("{} {} {}", self.name, self.counter, self.counter_type);
        }
        format!("{}{{{}}} {} {}", self.name, format_labels(&self.labels), self.counter, self.counter_type)
    }
}

fn format_labels(labels: &BTreeMap<String, String>) -> String {
    labels.iter().map(|(k, v)| format!("{}=\"{}\"", k, v)).collect::<Vec<_>>().join(",")
}
