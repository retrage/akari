#!/usr/bin/env make

SHELL := zsh

BUILD_TYPE := debug

VMM_NAME := server
ROOT_DIR := $(strip $(shell dirname $(realpath $(lastword $(MAKEFILE_LIST)))))
VMM_PATH := $(ROOT_DIR)/target/$(BUILD_TYPE)/$(VMM_NAME)

ENTITLEMENTS := runtime.entitlements
ENTITLEMENTS_PATH := $(ROOT_DIR)/$(ENTITLEMENTS)

CARGO ?= cargo
CODESIGN ?= codesign

build:
	$(CARGO) build && \
		$(CODESIGN) -f --entitlement $(ENTITLEMENTS_PATH) -s - $(VMM_PATH)

check:
	$(CARGO) check && \
		$(CARGO) fmt && \
		$(CARGO) clippy

.PHONY: build check

