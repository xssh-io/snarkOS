#!/bin/bash
# USAGE examples:
  # CLI with env vars: PROVER_PRIVATE_KEY=APrivateKey1...  ./run-prover.sh
  # CLI with prompts for vars:  ./run-prover.sh

# If the env var PROVER_PRIVATE_KEY is not set, prompt for it

CONFIG=~/.config/snarkOS/pool.json

# mainnet is not ready yet. use testnet
PEERS=$(scripts/get-testnet-nodes.sh)

COMMAND="snarkos start --nodisplay --pool --network 1 --config $CONFIG --peers $PEERS"

for word in $*;
do
  COMMAND="${COMMAND} ${word}"
done

echo "Running an Aleo Prover node..."
$COMMAND
