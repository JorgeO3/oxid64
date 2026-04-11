Findings
1. HIGH just verify-safety no representa la pila multicapa real y puede salir verde con capas críticas ausentes.
scripts/verify_safety.sh:39-46
scripts/verify_safety.sh:59-99
Justfile:363-369
doc/safety_verification.md:32-39
doc/safety_verification.md:79-83
doc/safety/baseline.md:76-78
El script por defecto solo ejecuta:
- cargo test
- un subconjunto pequeño de proptests x86
- Miri best-effort
- Kani best-effort
- MSan opcional
No ejecuta:
- cargo-careful
- ASan
- cargo-fuzz
- suites Miri por backend
- suites Kani explícitas por harness
- verificación runtime WASM
- checklist NEON
Además, por defecto verify_safety.sh solo emite WARN si faltan Miri/Kani/MSan. Incluso verify-safety-strict solo convierte esas ausencias en error; no añade las capas omitidas. Para un sistema crítico, tratar just verify-safety como “gate” principal daría falsa confianza.
2. HIGH La verificación de pointer arithmetic y safety de buffers es fuerte en helpers/modelos, pero no end-to-end sobre los kernels SIMD reales.
doc/safety/miri.md:5-24
doc/safety/kani.md:5-23
src/verify/mod.rs:67-165
src/verify/mod.rs:261-1191
Lo que sí está probado/revisado:
- aritmética helper dentro de una misma allocation
- guards can_read/can_write/can_advance
- safe_in_end_*
- offsets de stores modelados
- modelos de schedule y written-prefix
Lo que no está probado end-to-end:
- los loops SIMD reales con intrinsics
- la secuencia real de preloads/forward loads
- el acoplamiento completo entre prepare_decode_output y el slack de salida en cada iteración
- raw-reference formation UB dentro de kernels
Esto no es teórico: el bug real de SSSE3 align_output que encontramos en la auditoría anterior vive exactamente en ese hueco.
src/engine/ssse3.rs:462-475
3. HIGH La capa Miri/Kani que corre por defecto es materialmente más débil que la que el repo documenta como su perfil serio.
verify_safety.sh:69-82
Justfile:99-175
doc/safety/miri.md:25-63
doc/safety/kani.md:24-78
Miri en verify_safety.sh:
- corre solo cargo +nightly miri test --lib
- no usa -Zmiri-symbolic-alignment-check
- no usa -Zmiri-strict-provenance
- no corre tests/*_models.rs
- no corre test-miri-smoke
Kani en verify_safety.sh:
- corre cargo kani --tests
- no usa la lista explícita de harnesses mantenida en Justfile
- no preserva la granularidad de proofs por backend
Conclusión: una ejecución verde del script por defecto no equivale a la cobertura Miri/Kani que las docs describen como la pila seria.
4. HIGH La cobertura Miri no estaba posicionada para atrapar el UB de raw reference en SSSE3.
src/engine/ssse3.rs:472-475
Justfile:103-121
doc/safety/miri.md:7-16
doc/safety/miri.md:18-24
tests/ssse3_models.rs:1-136
Las suites Miri específicas de backend ejecutan modelos puros, no el path real de align_output() en ssse3.rs. Por eso una clase de UB importante para Rust, “crear una referencia fuera de rango desde raw pointer”, no estaba realmente instrumentada por la capa que debía ser la mejor para provenance/alignment.
5. MEDIUM Parte de Kani prueba consistencia interna o dominios muy pequeños, pero no prueba las propiedades peligrosas en el dominio real; por eso bugs reales pueden pasar.
src/verify/mod.rs:167-205
Dos ejemplos concretos:
- prepare_decode_output_matches_decoded_len_contract prueba prepare_decode_output() contra decoded_len_strict(), pero si decoded_len_strict() está semánticamente mal, el proof sigue verde.
- decoded_and_encoded_lengths_stay_bounded_in_small_domain usa raw_len <= 1024, así que no puede ver el overflow de encoded_len() cerca de usize::MAX.
Esto explica por qué la pila “formal” no detectó:
- la sobreaceptación de decoded_len_strict
- el riesgo de overflow en encoded_len
6. MEDIUM La capa “fuzz” del gate principal no es la capa de fuzz del repo; es un subconjunto pequeño x86.
scripts/verify_safety.sh:61-66
Justfile:235-357
doc/safety/fuzz.md:5-29
doc/safety/fuzz.md:31-92
tests/simd_fuzz_strict.rs:1-198
El repo sí tiene una familia amplia de cargo-fuzz, pero verify_safety.sh no corre ninguna. Solo escala un subconjunto de tests/proptest:
- sse_decode_tests
- avx2_decode_tests
- sse_encode_tests
- simd_fuzz_strict
Eso deja fuera del gate principal:
- AVX-512 fuzzing
- NEON fuzzing
- WASM fuzzing
- partial-write fuzzing específico
- unchecked-contract fuzzing
- encode fuzzing específico por backend fuera de SSE/AVX2
7. MEDIUM La cobertura de malformed padding es irregular y justamente ahí ya hay un bug real global.
tests/scalar_contracts.rs:63-97
tests/ssse3_models.rs:55-58
tests/avx2_models.rs:77-112
tests/avx512_vbmi_models.rs:114-117
tests/neon_models.rs:101-104
fuzz/fuzz_targets/ssse3_strict_diff.rs:22-30
fuzz/fuzz_targets/avx2_strict_diff.rs:22-30
fuzz/fuzz_targets/avx512_strict_diff.rs:26-34
fuzz/fuzz_targets/neon_strict_diff.rs:31-39
Scalar sí cubre malformed padding. AVX2 strict añade algunos tests internos con '='. Pero varias suites de backend y fuzzers saltan posiciones con =. Eso deja una laguna importante en una zona donde ya encontramos un fallo semántico real: los pad bits no canónicos del tail final.
8. MEDIUM WASM y parte de NEON están más cerca de “model-verified” que de “runtime-verified”.
doc/safety/fuzz.md:57-68
Justfile:30-45
Justfile:337-357
tests/wasm_simd128_decode_tests.rs:1-80
tests/wasm_simd128_encode_tests.rs:1-80
fuzz/fuzz_targets/wasm_pshufb_compat.rs:1-45
fuzz/fuzz_targets/wasm_non_strict_schedule.rs:1-73
fuzz/fuzz_targets/wasm_encode_prefix_model.rs:1-75
fuzz/fuzz_targets/wasm_partial_write_model.rs:1-57
doc/safety/baseline.md:29-65
Para WASM:
- el fuzzing es solo de modelos, no del backend runtime real
- el runtime real depende de wasmtime y +simd128
- el gate principal no lo corre
Para NEON:
- la pila existe en Justfile
- pero en el host actual la baseline muestra ausencia de target/tooling AArch64 operativo
- así que, en práctica, esa parte no forma parte de la verificación local cotidiana
9. LOW La cobertura pointer-adjacent es buena en varias rutas, pero no uniforme.
tests/scalar_contracts.rs:99-132
tests/sse_decode_tests.rs:161-199
tests/avx2_decode_tests.rs:203-244
tests/avx512_vbmi_decode_tests.rs:217-254
tests/wasm_simd128_decode_tests.rs:335-369
tests/neon_decode_tests.rs:277-329
tests/avx2_encode_tests.rs:1-43
Hay canary/exact-window tests valiosos para muchos paths, pero:
- NEON decode no tiene el mismo nivel de subslice/misaligned-input coverage que SSSE3/AVX2/AVX512/WASM
- AVX2 encode carece de una suite equivalente de exact-window/canary
10. LOW No hay automatización CI en el repo, y la propia policy de release exige prudencia sin CI verde.
doc/safety_verification.md:138-147
No encontré workflows en .github/workflows/. La policy ya dice que no se deben hacer claims absolutos sin pipelines reproducibles y verdes en CI. Hoy la pila es manual/local. Eso no invalida la verificación, pero sí aumenta riesgo de deriva entre docs, scripts y práctica real.
Qué está fuerte hoy
- La pila existe de verdad en el repo.
Justfile:17-175
Justfile:235-357
- Miri y MSan tienen smoke tests negativos, lo cual es excelente porque valida que la instrumentación no es placebo.
Justfile:95-97
Justfile:135-137
doc/safety/miri.md:55-64
doc/safety/msan.md:29-47
- Los helpers base de pointer arithmetic están bien pensados.
src/engine/common.rs:11-48
src/verify/mod.rs:67-132
tests/common_contracts.rs:21-89
remaining, can_read, can_write, safe_in_end_* evitan offset_from sobre punteros potencialmente problemáticos y sí tienen proofs dentro de una allocation.
- Hay model checking útil para store bounds.
src/verify/mod.rs:261-423
src/verify/mod.rs:426-675
src/verify/mod.rs:744-839
src/verify/mod.rs:973-1104
- Varias rutas usan constantes compartidas entre runtime y modelos, lo que reduce drift de thresholds.
src/engine/ssse3.rs
src/engine/avx2.rs
src/engine/neon.rs
src/engine/wasm_simd128.rs
Validación específica de pointer arithmetic
Cubierto razonablemente hoy:
- delta de punteros dentro de una misma allocation
- helpers de guardas y ventanas seguras
- offsets de store modelados bajo guards
- parte de exact-window/canary behavior en runtime tests
No cubierto de forma fuerte hoy:
- raw-reference formation desde raw pointers
- coupling real entre thresholds de input y headroom de output en cada iteración del kernel
- provenance bugs que no produzcan OOB inmediato
- misuse cross-allocation de helpers integerizados
- universal touched-prefix guarantees del kernel real
Qué capa atraparía qué:
- cargo-careful: precondiciones y contracts de std/unsafe; no reemplaza Miri/ASan
- ASan: OOB/UAF, pero no provenance; además detect_leaks=0
Justfile:47-68
- MSan: uninitialized reads, no pointer provenance
doc/safety/msan.md:14-20
- Miri: mejor herramienta para reference formation, alignment y provenance; hoy aplicada sobre helpers/modelos, no sobre todos los kernels reales
doc/safety/miri.md:5-24
- Kani: muy valioso para bounds/model contracts pequeños; no prueba kernels SIMD reales
doc/safety/kani.md:18-23
- cargo-fuzz/proptest: muy buenos para divergencias semánticas y crashes; no prueban soundness
Bottom line
La arquitectura de verificación es bastante mejor que la media de una librería SIMD manual. El problema no es “no hay stack”; el problema es que la puerta principal de verificación es demasiado débil comparada con la pila real, y la cobertura más profunda está concentrada en helpers y modelos, no en los kernels reales.
Para pointer arithmetic, mi conclusión es:
- common.rs está bien cubierto dentro de su contrato de uso
- la seguridad de los kernels sigue descansando en una mezcla de razonamiento manual, modelos, thresholds compartidos y tests con canarios
- la pila actual no justifica una claim de “pointer arithmetic exhaustivamente verificado end-to-end” para todos los backends
Si quieres, el siguiente paso natural es que te entregue un plan de endurecimiento por capas, priorizado para cerrar primero los huecos que habrían atrapado el bug real de SSSE3 y los bugs semánticos globales de decode.
