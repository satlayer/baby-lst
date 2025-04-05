# Cube by SatLayer

> BABY LST on Babylon Genesis

## Development

Install Rust: `https://www.rust-lang.org/tools/install`

### Building, Storing, and Instantiating Contracts

```bash
# Build and optimize WASM
./scripts/optimize.sh

# Store on chain
./scripts/store.sh

# Instantiate LST Token contract (example)
./scripts/instantiate.sh lst_token "CW20TestTokenLabel" 10000 '{"name":"testcw20","symbol":"cw20test","decimals":6,"hub_contract":"bbn1d676m90f8pn7hausxknwzt8ye9r6d4pfaud4muxmgx4p72fczplst4ds7w","marketing":null}'
```

### Scripts

Environment variables are defined in `./scripts/env.sh`:

```bash
export userKey="baby_wallet"        # Babylon wallet key
export nodeUrl="https://babylon-testnet-rpc.nodes.guru"
export chainId="bbn-test-5"
```

Note: Ensure proper environment setup before running scripts.

### `optimize.sh`

Builds and optimizes WASM files for workspace projects:

- Generates two files in `artifacts/` for each project:
  - `{project_name}_latest.wasm`
  - `{project_name}_{version}.wasm`
- Optimizes using wasm-opt (Binaryen v122)
- Enforces 800KB size limit
- Supports: lst_token, lst_reward_dispatcher, lst_validators_registry, lst_staking_hub

### `store.sh`

Stores optimized WASM files to Babylon blockchain:

- Uploads each contract to chain
- Records code IDs in `./scripts/code_ids.json`
- Requires: babylond, jq, git

### `instantiate.sh`

Instantiates stored contracts on Babylon blockchain:

- Gets code ID from `./scripts/code_ids.json` using project name and version
- Creates new contract instance from stored code
- Records contract addresses in `./scripts/contract_addresses.json`
- Usage: `./scripts/instantiate.sh PROJECT LABEL FEES INIT_JSON`
