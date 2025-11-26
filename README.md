# LightPlayer Compiler

This project provides a JIT compiler for running GLSL shaders on microcontrollers.

The architecture is inspired by [Cranelift](https://github.com/bytecodealliance/wasmtime/tree/main/cranelift), but is simpler and targets `no_std` environments.

A subset of GLSL is supported with the primary goal of running shaders to generate 2d graphics.

All arithmetic is done in 16.16 32-bit fixed-point.