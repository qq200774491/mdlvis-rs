#!/bin/bash

# Build and test parse mdlvis-rs with test model
set -e

echo "Building mdlvis-rs..."
cargo build --release

MODEL_PATH="test-data/Arthas.mdx"
MODEL_PATH="test-data/Ember Forge  Ember Knight/Ember Knight/Ember Knight_opt2.mdx"
MODEL_PATH="test-data/QDMR.mdx"

echo "Running with Arthas test model..."
./target/release/mdlvis-rs "$MODEL_PATH"