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

ARTIFACTS=("lst_token" "lst_reward_dispatcher" "lst_validators_registry" "lst_staking_hub")

get_wasm_with_version() {
    local contract="$1"
    local version=$(get_version "$contract")
    echo "${contract}_${version}".wasm
}

store_wasm () {
  local artifact_path="$REPO_PATH/artifacts"
  local artifact="$1"
  local artifact_with_version=$(get_wasm_with_version $artifact)

  echo "Storing $(basename $artifact_with_version)"
  res=$(babylond tx wasm store "$artifact_path/$artifact_with_version" --from $userKey --chain-id $chainId --gas 50000000 --fees=100000ubbn --node $nodeUrl -y -b sync -o "json")

  txhash=$(echo "$res" | jq -r '.txhash')
  echo "Transaction hash: $txhash"
  sleep 60
  code_id=$(babylond q tx $txhash -o json --node $nodeUrl | jq -r '.events[] | select(.type == "store_code").attributes[] | select(.key == "code_id").value')
  echo "Code ID: $code_id"

  json_file="$REPO_PATH/scripts/code_ids.json"

  if [ ! -f "$json_file" ]; then
      echo "{}" > "$json_file"
  fi

  # Set the code id in the json file
  filename=$(basename "$artifact_with_version")
  filename="${filename%.wasm}"
  jq --arg name "$filename" --arg id "$code_id" \
      '. + {($name): $id}' "$json_file" > "$json_file.tmp" && mv "$json_file.tmp" "$json_file"
}

for artifact in "${ARTIFACTS[@]}"; do
  store_wasm "$artifact"
done
