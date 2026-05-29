#!/bin/bash

# Do not run this script if you don't have at least 32 GiB of system memory.
# Submitting PRs that lack translated strings is okay if you don't meet the
# system requirements to run this script, or if you simply prefer not to; we are
# regularly updating the missing translation strings anyway.
#
# Base language: English (en.json)

set -e
cd "$(dirname "$0")"

bun install

# model URL:
# https://huggingface.co/unsloth/gemma-4-26B-A4B-it-GGUF
# you can specify your own if you want to.
if [ -z "${MODEL}" ]; then
	export MODEL="gemma-4-26B-A4B-it-UD-Q3_K_M"
fi

TEMPLATE="pl" bun main.ts
TEMPLATE="de" bun main.ts
TEMPLATE="ja" bun main.ts
TEMPLATE="es" bun main.ts
TEMPLATE="it" bun main.ts
TEMPLATE="zh_CN" bun main.ts
