use crate::handle::{PoolAddressResponse, SubmitSolutionRequest};
use crate::model::PuzzleResponse;
use anyhow::{bail, Context, Result};
use colored::Colorize;
use rand::rngs::OsRng;
use rand::{CryptoRng, Rng};
use reqwest::Url;
use serde_json::Value;
use snarkos_account::Account;
use snarkos_node_bft::helpers::fmt_id;
use snarkos_node_router::messages::NodeType;
use snarkvm::ledger::puzzle::{PartialSolution, Puzzle, Solution};
use snarkvm::ledger::store::ConsensusStorage;
use snarkvm::prelude::{Address, Network, PrivateKey, ViewKey, VM};
use std::marker::PhantomData;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, RwLock};

pub struct Worker<N: Network, C: ConsensusStorage<N>> {
    puzzle: Puzzle<N>,
    account: Account<N>,
    pool_address: Address<N>,
    pool_base_url: Url,
    client: reqwest::Client,
    shutdown: Arc<AtomicBool>,
    p: PhantomData<C>,
}
impl<N: Network, C: ConsensusStorage<N>> Worker<N, C> {
    pub async fn new(account: Account<N>, shutdown: Arc<AtomicBool>, pool_base_url: String) -> Result<Arc<Self>> {
        let puzzle = VM::<N, C>::new_puzzle()?;
        let client = reqwest::Client::new();
        let pool_base_url = pool_base_url.parse().with_context(|| "Invalid pool address")?;
        let pool_address = get_pool_address(&client, &pool_base_url).await?;
        let this =
            Arc::new(Self { puzzle, account, pool_address, pool_base_url, client, shutdown, p: Default::default() });
        tokio::spawn(this.clone().run());
        Ok(this)
    }
    /// Returns a solution to the puzzle.
    pub fn prove(&self, epoch_hash: N::BlockHash, address: Address<N>, counter: u64) -> Result<Solution<N>> {
        // Construct the partial solution.
        let partial_solution = PartialSolution::new(epoch_hash, address, counter)?;
        // Compute the proof target.
        let proof_target = self.puzzle.get_proof_target_from_partial_solution(&partial_solution)?;

        // Construct the solution.
        Ok(Solution::new(partial_solution, proof_target))
    }

    fn puzzle_iteration<R: Rng + CryptoRng>(
        &self,
        epoch_hash: N::BlockHash,
        coinbase_target: u64,
        proof_target: u64,
        rng: &mut R,
    ) -> Option<Solution<N>> {
        let address = self.pool_address;
        debug!(
            "Proving 'Puzzle' for Epoch '{}' {}",
            fmt_id(epoch_hash),
            format!("(Coinbase Target {coinbase_target}, Proof Target {proof_target})").dimmed()
        );
        // Compute the solution.
        match self.prove_puzzle(&self.puzzle, epoch_hash, address, rng.gen(), Some(proof_target)) {
            Ok(solution) => {
                info!(
                    "Proved 'Puzzle' for Epoch '{}' target={} {}",
                    fmt_id(epoch_hash),
                    solution.target(),
                    format!("(Coinbase Target {coinbase_target}, Proof Target {proof_target})").dimmed()
                );
                Some(solution)
            }
            Err(err) => {
                warn!("Failed to prove 'Puzzle': {}", err);
                None
            }
        }
    }
    /// Returns a solution to the puzzle.
    pub fn prove_puzzle(
        &self,
        puzzle: &Puzzle<N>,
        epoch_hash: N::BlockHash,
        address: Address<N>,
        counter: u64,
        _minimum_proof_target: Option<u64>,
    ) -> Result<Solution<N>> {
        // Construct the partial solution.
        let partial_solution = PartialSolution::new(epoch_hash, address, counter)?;
        // Compute the proof target.
        let proof_target = puzzle.get_proof_target_from_partial_solution(&partial_solution)?;
        // Check that the minimum proof target is met.

        // Construct the solution.
        Ok(Solution::new(partial_solution, proof_target))
    }
    pub async fn run(self: Arc<Self>) {
        let cpu_num = num_cpus::get();

        let task: Arc<RwLock<Option<PuzzleResponse>>> = Arc::new(RwLock::new(None));
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();

        for _ in 0..cpu_num {
            let task = task.clone();
            let worker = self.clone();
            let tx = tx.clone();
            rayon::spawn(move || {
                while !worker.shutdown.load(std::sync::atomic::Ordering::Acquire) {
                    if let Some(puzzle) = task.read().unwrap().clone() {
                        let epoch_hash: N::BlockHash = match puzzle.epoch_hash.parse() {
                            Ok(epoch_hash) => epoch_hash,
                            Err(_err) => {
                                warn!("Failed to parse epoch hash: {}", puzzle.epoch_hash);
                                continue;
                            }
                        };
                        let result =
                            worker.puzzle_iteration(epoch_hash, puzzle.coinbase_target, puzzle.difficulty, &mut OsRng);
                        if let Some(solution) = result {
                            if tx.send(solution).is_err() {
                                warn!("Failed to send solution");
                                break;
                            }
                        }
                    } else {
                        std::thread::sleep(std::time::Duration::from_secs(1));
                    }
                }
            });
        }
        {
            let client = self.client.clone();
            let pool_base_url = self.pool_base_url.clone();
            let task = task.clone();
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(3));
            tokio::spawn(async move {
                loop {
                    interval.tick().await;
                    let puzzle = match get_puzzle::<N>(&client, &pool_base_url).await {
                        Ok(puzzle) => puzzle,
                        Err(err) => {
                            warn!("Failed to get puzzle: {:?}", err);
                            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                            continue;
                        }
                    };
                    if let Some(task) = &*task.read().unwrap() {
                        if task.block_round == puzzle.block_round {
                            continue;
                        }
                    }
                    info!("New puzzle: {:?}", puzzle);
                    *task.write().unwrap() = Some(puzzle.clone());
                }
            });
        }
        while !self.shutdown.load(std::sync::atomic::Ordering::Acquire) {
            let Some(solution) = rx.recv().await else {
                warn!("Failed to receive solution");
                break;
            };

            let client = self.client.clone();
            let pool_base_url = self.pool_base_url.clone();
            let address = self.account.address();
            let block_round = task.read().unwrap().as_ref().unwrap().block_round;
            if let Err(err) = submit_solution(&client, &address, &pool_base_url, solution, block_round).await {
                warn!("Failed to submit solution: {:?}", err);
            }
        }
    }
    pub fn node_type(&self) -> NodeType {
        NodeType::Worker
    }
    pub fn private_key(&self) -> &PrivateKey<N> {
        self.account.private_key()
    }
    pub fn view_key(&self) -> &ViewKey<N> {
        self.account.view_key()
    }
    pub fn address(&self) -> Address<N> {
        self.account.address()
    }
    pub fn is_dev(&self) -> bool {
        false
    }
}
pub async fn get_puzzle<N: Network>(client: &reqwest::Client, pool_base_url: &Url) -> Result<PuzzleResponse> {
    let response = client.get(format!("{}/puzzle", pool_base_url)).send().await?;
    let status = response.status();
    if !status.is_success() {
        let text = response.text().await?;
        bail!("Failed to get puzzle: {} {}", status, text);
    }
    let resp: PuzzleResponse = response.json().await?;
    Ok(resp)
}
pub async fn get_pool_address<N: Network>(client: &reqwest::Client, pool_base_url: &Url) -> Result<Address<N>> {
    let response = client.get(format!("{}/pool_address", pool_base_url)).send().await?;
    let status = response.status();
    if !status.is_success() {
        let text = response.text().await?;
        bail!("Failed to get pool address: {} {}", status, text);
    }
    let resp: PoolAddressResponse = response.json().await?;
    let address = resp.pool_address.parse().with_context(|| format!("Invalid address: {}", resp.pool_address))?;
    Ok(address)
}
pub async fn submit_solution<N: Network>(
    client: &reqwest::Client,
    address: &Address<N>,
    pool_base_url: &Url,
    solution: Solution<N>,
    block_round: u64,
) -> Result<()> {
    info!("Submitting solution: {}", solution.target());
    let response = client
        .post(format!("{}/solution", pool_base_url))
        .json(&SubmitSolutionRequest {
            address: address.to_string(),
            solution: solution.into(),
            block_round,
            verified: false,
        })
        .send()
        .await?;
    let status = response.status();
    if !status.is_success() {
        let text = response.text().await?;
        bail!("Failed to submit solution: {} {}", status, text);
    }
    let resp: Value = response.json().await?;
    info!("Response: {}", resp);
    Ok(())
}
