#!/bin/bash
# USAGE examples:
# CLI with env vars: PROVER_PRIVATE_KEY=APrivateKey1...  ./run-prover.sh
# CLI with prompts for vars:  ./run-prover.sh

# If the env var PROVER_PRIVATE_KEY is not set, prompt for it

CONFIG=~/.config/snarkOS/pool.json
if [ -z "${PROVER_PRIVATE_KEY}" ]
then
  read -r -p "Enter the Aleo Prover account private key: "
  PROVER_PRIVATE_KEY=$REPLY
fi

if [ "${PROVER_PRIVATE_KEY}" == "" ]
then
  echo "Missing account private key. (run 'snarkos account new' and try again)"
  exit
fi

# mainnet is not ready yet. use testnet
PEERS=$(scripts/get-testnet-nodes.sh)

COMMAND="snarkos start --nodisplay --pool --network 1 --config $CONFIG --peers $PEERS"

for word in $*; do
  COMMAND="${COMMAND} ${word}"
done

echo "Running an Aleo Prover node..."
$COMMAND | tee -a ~/snarkos-pool.log
