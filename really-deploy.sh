#!/usr/bin/env bash

dx bundle --package web --release

./deploy.sh d1zf3x165bl7db target/dx/web/release/web/public 
