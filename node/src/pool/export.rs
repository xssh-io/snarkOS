use anyhow::Result;

use crate::handle::SubmitSolutionRequest;

#[async_trait::async_trait]
pub trait ExportSolution: Send + Sync {
    fn export_solution(&self, solution: &SubmitSolutionRequest) -> Result<()>;
}

#[async_trait::async_trait]
impl ExportSolution for () {
    fn export_solution(&self, _solution: &SubmitSolutionRequest) -> Result<()> {
        Ok(())
    }
}

pub struct ExportSolutionClickhouse {
    client: clickhouse_rs::Client,
}
impl ExportSolutionClickhouse {
    pub fn new(client: clickhouse_rs::Client) -> Self {
        Self { client }
    }
    pub async fn export_solution(&self, solution: &SubmitSolutionRequest) -> Result<()> {
        let query = format!(
            "INSERT INTO solutions (id, problem_id, user_id, language, code, created_at, updated_at) VALUES ({}, {}, {}, {}, {}, {}, {})",
            solution.id,
            solution.problem_id,
            solution.user_id,
            solution.language,
            solution.code,
            solution.created_at,
            solution.updated_at,
        );
        self.client.execute(query).await?;
        Ok(())
    }
}

#[async_trait::async_trait]
impl ExportSolution for ExportSolutionClickhouse {
    fn export_solution(&self, solution: &SubmitSolutionRequest) -> Result<()> {
        for export in self {
            export.export_solution(solution)?;
        }
        Ok(())
    }
}
