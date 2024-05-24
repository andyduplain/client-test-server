#!/bin/bash
export PATH=$HOME/.cargo/bin:$PATH
(cd $(dirname $0); exec cargo run --release)
