#!/usr/bin/env bash

dx bundle --package mobile --android --target aarch64-linux-android --package-types "apk" --release

adb -s R5CRB1J2TRB install ./target/dx/mobile/release/android/app/app/build/outputs/apk/debug/app-debug.apk
