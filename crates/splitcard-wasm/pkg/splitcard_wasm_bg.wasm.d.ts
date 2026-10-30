/* tslint:disable */
/* eslint-disable */
export const memory: WebAssembly.Memory;
export const card_hash: (a: number, b: number) => [number, number, number, number];
export const card_status: (a: number, b: number, c: number, d: number) => [number, number, number];
export const check_supersede: (a: number, b: number, c: number, d: number, e: number, f: number) => [number, number];
export const compute_payouts: (a: number, b: number, c: number, d: number) => [number, number, number];
export const display_basis_points: (a: number, b: number) => [number, number, number];
export const parse_secret_key: (a: number, b: number) => [number, number, number, number];
export const public_key: (a: number, b: number) => [number, number, number, number];
export const secret_key_nsec: (a: number, b: number) => [number, number, number, number];
export const sign_event: (a: number, b: number, c: number, d: number, e: number, f: number) => [number, number, number, number];
export const signature_template: (a: number, b: number, c: number, d: number, e: any) => [number, number, number];
export const validate_card: (a: number, b: number) => [number, number];
export const verify_signature: (a: number, b: number, c: number, d: number) => [number, number, number, number];
export const version: () => [number, number];
export const rustsecp256k1_v0_10_0_context_create: (a: number) => number;
export const rustsecp256k1_v0_10_0_context_destroy: (a: number) => void;
export const rustsecp256k1_v0_10_0_default_error_callback_fn: (a: number, b: number) => void;
export const rustsecp256k1_v0_10_0_default_illegal_callback_fn: (a: number, b: number) => void;
export const __wbindgen_malloc: (a: number, b: number) => number;
export const __wbindgen_realloc: (a: number, b: number, c: number, d: number) => number;
export const __wbindgen_externrefs: WebAssembly.Table;
export const __externref_table_dealloc: (a: number) => void;
export const __wbindgen_free: (a: number, b: number, c: number) => void;
export const __wbindgen_start: () => void;
