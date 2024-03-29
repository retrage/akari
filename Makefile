#!/usr/bin/env make

SHELL := zsh

BUILD_TYPE := debug

BIN_NAME := akari
ROOT_DIR := $(strip $(shell dirname $(realpath $(lastword $(MAKEFILE_LIST)))))
BIN_PATH := $(ROOT_DIR)/target/$(BUILD_TYPE)/$(BIN_NAME)

ENTITLEMENTS := runtime.entitlements
ENTITLEMENTS_PATH := $(ROOT_DIR)/$(ENTITLEMENTS)

CARGO ?= cargo
CODESIGN ?= codesign

build:
	$(CARGO) build && \
		$(CODESIGN) -f --entitlement $(ENTITLEMENTS_PATH) -s - $(BIN_PATH)

.PHONY: build

