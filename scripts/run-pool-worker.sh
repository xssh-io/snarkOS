#!/bin/bash
# USAGE examples:
  # CLI with env vars: PROVER_PRIVATE_KEY=APrivateKey1...  ./run-prover.sh
  # CLI with prompts for vars:  ./run-prover.sh

# If the env var PROVER_PRIVATE_KEY is not set, prompt for it
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

if [ "${POOL_BASE_URL}" == "" ]
then
  echo "missing POOL_BASE_URL. run with POOL_BASE_URL=http://address:3031 ./run-prover.sh"
  exit
fi

PEERS=$(scripts/get-testnet-nodes.sh)

COMMAND="snarkos start --nodisplay --prover --network 1 --private-key ${PROVER_PRIVATE_KEY} --pool-base-url ${POOL_BASE_URL} --peers $PEERS"

for word in $*;
do
  COMMAND="${COMMAND} ${word}"
done

echo "Running an Aleo Prover node..."
$COMMAND

