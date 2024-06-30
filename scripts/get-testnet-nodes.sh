#!/bin/bash

JSON=$(curl http://35.231.152.213:3030/testnet/peers/all)
COMMA_SPT=$(echo "$JSON" | jq -r '.[]' | tr '\n' ',')
COMMA_SPT=${COMMA_SPT%,}
echo -n "$COMMA_SPT"
