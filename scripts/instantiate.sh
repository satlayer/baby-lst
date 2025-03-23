#!/bin/bash

set -euo pipefail

for cmd in babylond jq git; do
  if ! command -v $cmd &> /dev/null; then
    echo "Note: $cmd is required but not installed"
  fi
done

REPO_PATH=$(git rev-parse --show-toplevel)

source $REPO_PATH/scripts/env.sh
source $REPO_PATH/scripts/helper.sh

cd $REPO_PATH

PROJECT=$1
LABEL=$2
FEES=$3
INIT_JSON=$4

get_code_id() {
    local project=$1
    local version=$(get_version "$project")
    local project=${project}_${version}
    local code_id=$(jq -r ".[\"$project\"]" "${REPO_PATH}/scripts/code_ids.json")
    
    if [ "$code_id" == "null" ] || [ -z "$code_id" ]; then
        echo "Error: Project '$project' not found in code_ids.json"
        exit 1
    fi
    echo $code_id
}

CODE_ID=$(get_code_id "$PROJECT")
echo "Using Code ID: $CODE_ID"

res=$(babylond tx wasm instantiate $CODE_ID "$INIT_JSON" --from=$userKey --admin=$userKey --label="test-label" --gas=2000000 --fees="${FEES}ubbn" --chain-id=$chainId -b=sync -y --log_format=json -o "json" --node $nodeUrl)
txhash=$(echo "$res" | jq -r '.txhash')
echo "Transaction hash: $txhash"
sleep 45
address=$(babylond q tx "$txhash" -o json --node "$nodeUrl" | jq -r '.events[] | select(.type == "instantiate").attributes[] | select(.key == "_contract_address").value')

echo "Contract address: $address"

json_file="${REPO_PATH}/scripts/contract_addresses.json"

if [ ! -f "$json_file" ]; then
    echo "{}" > "$json_file"
fi

filename=$(basename "$PROJECT" .wasm)
jq --arg name "$filename" --arg id "$address" \
    '. + {($name): $id}' "$json_file" > "$json_file.tmp" && mv "$json_file.tmp" "$json_file"