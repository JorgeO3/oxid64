
# Plan paranoico de correctness/UB para `oxid64`

**Objetivo:** convertir `oxid64` en un engine Base64 de grado industrial con una defensa en profundidad específica para `unsafe`, SIMD, aritmética de punteros, overstores controlados, paths de cola, dispatch por `target_feature`, y divergencias entre backends.

**Stack objetivo**

1. `cargo-careful` — red barata, rápida y frecuente  
2. ASan — primera barrera dinámica grande  
3. MSan — barrera especializada para lecturas no inicializadas  
4. Miri — bisturí para UB semántico y de modelo de memoria  
5. Kani — verificación exhaustiva de helpers/contratos  
6. `cargo-fuzz` — generación masiva de inputs malditos / differential testing  

**Herramientas de apoyo (muy recomendadas, aunque no estén en el stack principal):**

- `cargo-geiger` — inventario y presupuesto de `unsafe`
- `unsafe_op_in_unsafe_fn` + `clippy::undocumented_unsafe_blocks` — disciplina obligatoria
- `-Zrandomize-layout` — detectar dependencia accidental de layout
- LSan/TSan/Loom — opcionales según expansión futura

---

## 0. Alcance y observaciones del repositorio

### Árbol reportado

Tu repositorio tiene:

- `src/engine/{scalar,ssse3,avx2,avx512vbmi,neon,wasm_simd128}.rs`
- tests por backend (`sse_*`, `avx2_*`, `avx512_*`, `neon_*`, `wasm_*`)
- property tests (`proptest.rs`)
- un test/fuzz estricto SIMD (`simd_fuzz_strict.rs`)
- scripts y documentación técnica

### Lo que pude inspeccionar directamente

El comprimido adjunto que pude abrir contenía los archivos de `src/engine/*` (no el repo completo), así que las observaciones de bajo nivel están basadas en esos módulos, y el resto del árbol lo tomo de tu listado manual.

### Superficie de riesgo actual

En la copia de `src/engine/*` que pude revisar:

- `engine/avx2.rs`: **1088** líneas
- `engine/avx512vbmi.rs`: **696** líneas
- `engine/mod.rs`: **341** líneas
- `engine/neon.rs`: **543** líneas
- `engine/scalar.rs`: **867** líneas
- `engine/ssse3.rs`: **882** líneas
- `engine/wasm_simd128.rs`: **840** líneas

Conteo aproximado de `unsafe fn` / `unsafe` / `target_feature`:

| Archivo | `unsafe fn` | `unsafe` (kw) | `target_feature` | `.add(` |
|---|---:|---:|---:|---:|
| `avx2.rs` | 21 | 23 | 21 | 104 |
| `avx512vbmi.rs` | 12 | 13 | 10 | 59 |
| `mod.rs` | 1 | 2 | 3 | 0 |
| `neon.rs` | 11 | 12 | 11 | 34 |
| `scalar.rs` | 2 | 7 | 0 | 1 |
| `ssse3.rs` | 17 | 19 | 25 | 72 |
| `wasm_simd128.rs` | 19 | 21 | 23 | 62 |

### Lectura operativa de estas cifras

Esto es bueno y malo al mismo tiempo:

- **Bueno:** el riesgo está muy concentrado en `src/engine/*`; no tienes `unsafe` desperdigado por todo el crate.
- **Malo:** la densidad de punteros, offsets, loads/stores y guardas manuales es alta. Ahí vive el UB elegante y silencioso.

### Riesgos dominantes que este stack debe cazar

1. **Construcción de punteros fuera de allocation**
   - `ptr.add(n)` usado como guard
   - preloads “optimistas” antes de comprobar suficientes bytes
   - offsets negativos/solapados para encode overlap paths

2. **Lecturas o stores fuera de rango**
   - loads de 16/32/64 bytes
   - overstores deliberados de 16/32/64 bytes con solo 12/24/48 bytes “meaningful”
   - tail drains y slack mal calculados

3. **Alineación**
   - casts a `*const __m128i/__m256i/__m512i`
   - paths que asumen alineación o la persiguen de forma incorrecta
   - `read()`/`write()` sobre tipos con alineación mayor

4. **Lecturas no inicializadas**
   - lanes parciales
   - `MaybeUninit`
   - paths híbridos scalar+SIMD
   - buffers temporales con bytes “basura pero aparentemente inofensivos”

5. **Violaciones de contrato**
   - wrappers públicos que prometen “None si el input es inválido”, pero ciertos modos fast-path no validan todo
   - diferencias semánticas entre strict/non-strict
   - diferencias entre scalar y backends SIMD

6. **Divergencia funcional entre backends**
   - scalar vs SSSE3/AVX2/AVX512/NEON/WASM
   - encode/decode roundtrip
   - tratamiento de tails, padding, inputs inválidos y tamaños frontera

---

## 1. Filosofía de la industrialización

La idea central es esta:

> **No intentes verificar el core SIMD “monolítico” de golpe.**
> Divide el problema en **contratos pequeños**, centraliza las precondiciones, y deja que cada herramienta muerda la parte donde es más fuerte.

### Regla de oro

Separar cada backend en dos capas:

1. **Capa verificable**
   - guards
   - cálculo de slack
   - helpers de punteros
   - precondiciones de loads/stores
   - aritmética de offsets
   - preámbulos de alineación
   - determinación de límites seguros

2. **Capa de intrinsics**
   - loads/stores reales
   - `pshufb`, `pmaddubsw`, `vqtbl`, etc.
   - packing/unpacking vectorial

### Meta

Toda función `unsafe` debe tener:

- precondiciones **explícitas y pequeñas**
- un comentario `// SAFETY:` verificable
- un helper nombrado para los invariantes importantes

Ejemplos de helpers que deben existir:

- `remaining(in_ptr, in_end) -> usize`
- `remaining_mut(out_ptr, out_end) -> usize`
- `can_read(ptr, end, n) -> bool`
- `can_write(ptr, end, n) -> bool`
- `can_overstore(ptr, end, meaningful, overstore) -> bool`
- `advance_checked(ptr, end, n) -> *const u8`
- `advance_checked_mut(ptr, end, n) -> *mut u8`
- `safe_tail_start(len, width) -> usize`
- `safe_in_end_for_4(input) -> *const u8`
- `safe_in_end_for_16(input) -> *const u8`
- `safe_in_end_for_32(input) -> *const u8`
- `safe_in_end_for_64(input) -> *const u8`

Estos helpers no son “boilerplate feo”: son la materia prima que Kani y Miri pueden verificar.

---

## 2. Cambios estructurales previos al stack (imprescindibles)

## 2.1. Disciplina del `unsafe`

### Requisitos

- Activar `unsafe_op_in_unsafe_fn`
- Activar `clippy::undocumented_unsafe_blocks`
- Exigir comentarios `// SAFETY:` encima de cada bloque `unsafe`
- Minimizar el alcance de cada `unsafe {}`

### Recomendación en `lib.rs` o `src/engine/mod.rs`

```rust
#![deny(unsafe_op_in_unsafe_fn)]
#![deny(clippy::undocumented_unsafe_blocks)]
#![warn(clippy::cast_ptr_alignment)]
#![warn(clippy::ptr_as_ptr)]
#![warn(clippy::transmute_ptr_to_ptr)]
```

### Política

- **No** `#[allow(unsafe_op_in_unsafe_fn)]` a nivel de módulo salvo un motivo técnico muy sólido.
- Los intrinsics deben quedar dentro de bloques pequeños.
- Todo guard de límites debe estar fuera del `unsafe` cuando sea posible.

---

## 2.2. Factorizar contratos comunes

Crear un módulo común, por ejemplo:

```text
src/
  engine/
    common/
      bounds.rs
      ptr.rs
      slack.rs
      tail.rs
      contracts.rs
      verify.rs
```

### Qué debe ir ahí

#### `bounds.rs`
- `remaining`
- `can_read`
- `can_write`
- `can_overstore`

#### `ptr.rs`
- `advance_checked`
- `advance_checked_mut`
- `offsets_checked`
- wrappers para pointer arithmetic

#### `slack.rs`
- helpers para `meaningful + overstore`
- helpers por ancho: 4/16/32/64
- helpers para bloques DS64/DS128/DS256

#### `tail.rs`
- `safe_in_end_for_4`
- `safe_in_end_for_vector(width)`

#### `contracts.rs`
- invariantes documentados por backend
- macros/helpers para asserts de runtime en debug

### Beneficio

- `cargo-careful`, Miri y Kani muerden estos helpers con mucha mejor precisión.
- Reduces duplicación entre SSSE3 / AVX2 / AVX512 / NEON / WASM.
- Vuelves explícita la semántica de overstores y preload slack.

---

## 2.3. Wrappers checked para loads/stores

Crear wrappers de bajo nivel que documenten contrato:

```rust
unsafe fn loadu128_checked(ptr: *const u8, end: *const u8) -> __m128i
unsafe fn storeu128_checked(ptr: *mut u8, end: *mut u8, v: __m128i)

unsafe fn loadu256_checked(ptr: *const u8, end: *const u8) -> __m256i
unsafe fn storeu256_checked(ptr: *mut u8, end: *mut u8, v: __m256i)

unsafe fn loadu512_checked(ptr: *const u8, end: *const u8) -> __m512i
unsafe fn storeu512_checked(ptr: *mut u8, end: *mut u8, v: __m512i)
```

Versiones específicas para overstore:

```rust
unsafe fn storeu128_overstore_checked(
    ptr: *mut u8,
    end: *mut u8,
    meaningful: usize,
    overstore: usize,
    v: __m128i,
)
```

### Regla

**Ningún** `_mm*_loadu_*` o `_mm*_storeu_*` directo en loops principales hasta que existan wrappers con contratos centralizados.

---

## 2.4. Instrumentación específica para Miri y Kani

Usar `cfg(miri)` y `cfg(kani)` para añadir checks extra, no para cambiar semántica completa.

Ejemplo:

```rust
#[cfg(any(miri, kani))]
#[inline]
fn debug_require(cond: bool, msg: &str) {
    assert!(cond, "{msg}");
}

#[cfg(not(any(miri, kani)))]
#[inline]
fn debug_require(_cond: bool, _msg: &str) {}
```

Luego:

```rust
debug_require(can_read(in_ptr, in_end, 16), "need 16 input bytes");
```

Eso hace a Miri y a los tests de contrato mucho más informativos.

---

## 3. Estrategia por herramienta

# 3.1. `cargo-careful`

## Rol en el stack
Primera red de CI barata y frecuente.

## Qué aporta
- Recompila `std` con debug assertions.
- Activa checks adicionales de UB en runtime.
- Detecta alineación/no-null en `ptr.read()`/`ptr.write()`.
- Añade checks más estrictos a `mem::zeroed` / `mem::uninitialized`.
- Es mucho más rápido y menos quisquilloso que Miri.

## Dónde usarlo en `oxid64`
- En toda la suite de tests del crate
- En tests de unidad de helpers comunes
- En tests de contratos (`tests/contracts_*`)
- En pruebas de scalar y wrappers checked
- Idealmente también sobre examples y doctests

## Qué NO esperar
- No sustituye Miri.
- No detecta toda forma de UB.
- No te prueba exhaustivamente condiciones simbólicas.

## Integración concreta

### Instalación
```bash
cargo install cargo-careful
```

### Comandos base
```bash
cargo +nightly careful test
cargo +nightly careful test --lib
cargo +nightly careful test --tests
cargo +nightly careful test --doc
```

### Uso recomendado en CI
- correr en cada PR
- bloquear merge si falla

### Meta operativa
Que cualquier bug obvio de alignment / ptr.read()/write() / invariantes internas salte aquí antes de llegar a Miri.

---

# 3.2. ASan

## Rol en el stack
Primera barrera dinámica fuerte para memoria.

## Qué aporta
- OOB
- use-after-free
- use-after-scope
- double-free
- gran señal para punteros y overstores
- puede convivir con código no instrumentado, aunque con menor cobertura

## Dónde usarlo
- Suite completa del crate en Linux x86_64
- Tests de integración por backend
- Differential tests
- Fuzzing (libFuzzer lo usa muy bien)
- Self-hosted / cloud runners para targets con AVX512 si quieres cazar bugs del path real

## Target base recomendado
`x86_64-unknown-linux-gnu`

## Comandos base
```bash
RUSTFLAGS="-Zsanitizer=address" \
cargo +nightly test -Zbuild-std --target x86_64-unknown-linux-gnu
```

### Recomendación adicional
Incluir tests con buffers desalineados y subslices:
- `&mut out[1..]`
- `&mut out[3..]`
- `&mut out[15..]`

ASan es especialmente útil para matar overstores mal presupuestados.

## Plan de adopción
1. Correrlo **sin refactor** para ver qué explota ya.
2. Introducir wrappers checked y volverlo verde.
3. Mantenerlo en CI en cada PR.

---

# 3.3. MSan

## Rol en el stack
Barrera especializada para lecturas de memoria no inicializada.

## Qué aporta
- detecta lecturas de bytes/lanes no inicializados
- muy útil para `MaybeUninit`
- muy útil para temporales parciales y paths híbridos scalar+SIMD

## Coste / fricción
- requiere instrumentación completa del programa
- C/C++ dependencias deben recompilarse con `-fsanitize=memory`
- si no, aparecen falsos positivos

## Dónde usarlo en `oxid64`
- especialmente en:
  - scalar core
  - tail handlers
  - wrappers de store/load
  - encode/decode con temporales
  - differential tests
  - fuzzing dirigido a tails y padding

## Comando base
```bash
RUSTFLAGS="-Zsanitizer=memory -Zsanitizer-memory-track-origins" \
RUSTDOCFLAGS="-Zsanitizer=memory -Zsanitizer-memory-track-origins" \
cargo +nightly test -Zbuild-std --target x86_64-unknown-linux-gnu
```

## Estrategia correcta
- no ponerlo en cada PR al principio
- usarlo en nightly CI o en workflow manual
- después de que ASan esté verde

## Observación importante
MSan vale muchísimo para `oxid64` porque una librería SIMD puede “funcionar” leyendo basura cósmica que casualmente no altera el resultado en ciertos casos. Ese bug es exactamente el tipo de humillación que MSan disfruta exponer.

---

# 3.4. Miri

## Rol en el stack
Bisturí para UB semántico.

## Qué aporta
- OOB
- use-after-free
- uninitialized memory
- alignment
- validity de tipos
- aliasing experimental
- provenance
- data races y algo de weak memory
- múltiples seeds
- cross-interpretation (por ejemplo big-endian)

## Limitación central
No es la herramienta para ejecutar cómodamente todos los intrinsics SIMD de todas tus rutas reales.

## Cómo usarlo bien en `oxid64`
**No** usar Miri como “ejecutor del backend AVX2 entero”.  
**Sí** usar Miri para:

1. helpers comunes
2. preámbulos de alineación
3. cálculo de safe ends
4. wrappers checked
5. scalar core
6. tests de contratos
7. modelos pequeños de los backends

## Flags recomendadas

### perfil base
```bash
MIRIFLAGS="-Zmiri-backtrace=full -Zmiri-symbolic-alignment-check -Zmiri-strict-provenance" \
cargo +nightly miri test
```

### múltiples seeds
```bash
MIRIFLAGS="-Zmiri-backtrace=full -Zmiri-symbolic-alignment-check -Zmiri-many-seeds=0..16" \
cargo +nightly miri test
```

### big-endian
```bash
MIRIFLAGS="-Zmiri-backtrace=full" \
cargo +nightly miri test --target s390x-unknown-linux-gnu
```

## Tests que deben existir para Miri

### carpeta sugerida
```text
tests/miri/
  bounds.rs
  ptr_math.rs
  overstore.rs
  align.rs
  scalar_tails.rs
  dispatch_contracts.rs
```

### tipos de tests
- `safe_in_end_*`
- `advance_checked_*`
- `can_overstore_*`
- paths pequeños con `out` desalineado
- inputs 0..=N
- bordes de padding
- pruebas negativas de contrato

## Uso de `cfg(miri)`
Solo para:
- ignorar tests no soportados
- añadir asserts extra

No usar para “reemplazar todo SSSE3 por scalar” si el objetivo era validar la ruta real. Mejor factorizar helpers compartidos y probar esos.

---

# 3.5. Kani

## Rol en el stack
Verificación exhaustiva de helpers/contratos pequeños.

## Qué aporta
- model checking bit-precise
- muy fuerte para bounds, arithmetic, guards, loops pequeños, precondiciones
- excelente para `unsafe` de frontera

## Limitación central
No es práctico intentar probar el módulo SIMD entero con intrinsics y loops grandes de una sola vez.

## Dónde usarlo en `oxid64`

### Debe verificar:
- `remaining`
- `can_read`
- `can_write`
- `can_overstore`
- `advance_checked`
- `offsets_checked`
- `safe_in_end_for_4/16/32/64`
- helpers de tail
- cálculos de longitud
- helpers de padding
- contratos de wrappers checked

### No debe ser el primer objetivo:
- `process_ds64_*`
- `process_ds128_*`
- `process_ds256_*`
- packing intrinsics reales

## Estructura sugerida

```text
src/
  verify/
    mod.rs
    bounds_kani.rs
    ptr_kani.rs
    slack_kani.rs
    scalar_contracts_kani.rs
```

o dentro de cada módulo bajo `#[cfg(kani)]`.

## Comandos base
```bash
cargo kani
cargo kani --harness safe_in_end_within_bounds
cargo kani --harness advance_checked_never_oob
cargo kani --default-unwind 8
cargo kani --tests
```

## Harnesses mínimos obligatorios

1. `advance_checked_never_oob`
2. `advance_checked_mut_never_oob`
3. `safe_in_end_for_4_within_allocation`
4. `safe_in_end_for_16_within_allocation`
5. `guard_12_plus_4_implies_store_is_safe`
6. `guard_24_plus_8_implies_store_is_safe`
7. `guard_48_plus_16_implies_store_is_safe`
8. `decoded_len_strict_never_overflows`
9. `encoded_len_never_overflows`
10. `offsets_checked_monotonic`

## Uso realista en CI
- no correr todos los harnesses siempre al principio
- ejecutar un conjunto chico y crítico por PR
- suite más completa nightly

---

# 3.6. `cargo-fuzz`

## Rol en el stack
Generador industrial de inputs malditos.

## Qué aporta
- encuentra divergencias funcionales
- combinado con sanitizers revienta UB real
- ideal para differential testing entre backends
- excelente para tamaños pequeños, tails, padding y casos inválidos

## Qué debe fuzzear `oxid64`

### 1. Differential decode
Comparar:
- scalar strict
- ssse3 strict
- avx2 strict
- avx512 strict
- neon strict
- wasm strict

si el target/backend está disponible.

### 2. Differential encode
Comparar:
- scalar encode
- ssse3 encode
- avx2 encode
- avx512 encode
- neon encode
- wasm encode

### 3. Roundtrip
- `decode(encode(x)) == x`

### 4. Invalid-input semantics
- si scalar devuelve `None`, el backend equivalente strict debe devolver `None`
- registrar explícitamente si non-strict permite más cosas y documentarlo

### 5. Buffer alignment chaos
Generar:
- `input` con offsets
- `out` con offsets 0..15
- slices recortadas y desalineadas

## Estructura sugerida

```text
fuzz/
  Cargo.toml
  fuzz_targets/
    decode_diff.rs
    encode_diff.rs
    roundtrip.rs
    invalid_semantics.rs
    tail_alignment.rs
    backend_pair_ssse3_vs_scalar.rs
    backend_pair_avx2_vs_scalar.rs
    backend_pair_avx512_vs_scalar.rs
```

## Comandos
```bash
cargo fuzz init
cargo fuzz run decode_diff
cargo fuzz run encode_diff
cargo fuzz run tail_alignment
```

## Regla crucial
Los fuzz targets deben tener **oráculos claros**:

- scalar como referencia funcional
- invariantes explícitas sobre longitud y retorno
- asserts pequeños y precisos

## Integración con sanitizers
Muy recomendable correr fuzzing con ASan; opcionalmente algunos jobs con MSan.

---

## 4. Herramientas adicionales muy recomendadas

# 4.1. `cargo-geiger`

## Rol
Inventario de `unsafe`, no detector de soundness.

## Uso en `oxid64`
- obtener línea base de radiación
- rastrear crecimiento de `unsafe` propio y de dependencias
- generar reportes por PR / release

## Comandos
```bash
cargo install --locked cargo-geiger
cargo geiger
cargo geiger --output-format Json
```

## Política
- guardar baseline
- no aumentar recuento de `unsafe` sin justificarlo en el PR
- todo nuevo `unsafe` debe enlazar a un contrato/documentación

---

# 4.2. `-Zrandomize-layout`

## Rol
Destapar dependencia accidental de `repr(Rust)` o padding/layout implícito.

## Uso
No es tu primera línea, pero sí una buena prueba nocturna.

```bash
RUSTFLAGS="-Zrandomize-layout" cargo +nightly test
```

Útil especialmente si introdujiste structs auxiliares o empaquetados temporales raros.

---

# 4.3. LSan

## Rol
LeakSanitizer para detectar leaks.

## Prioridad
Media-baja para `oxid64`, pero casi gratis si ya tienes sanitizers.

## Uso
```bash
RUSTFLAGS="-Zsanitizer=leak" \
cargo +nightly test -Zbuild-std --target x86_64-unknown-linux-gnu
```

---

# 4.4. TSan / Loom

## Hoy
Probablemente no prioritario si `oxid64` es mayormente stateless.

## Cuándo activarlos
Si metes:
- caches globales
- dispatch lazy
- tablas mutables globales
- inicialización compartida
- benchmark harnesses con estado compartido
- pools / arenas globales

### TSan
Para data races reales en runtime.

### Loom
Para permutar ejecuciones si algún día introduces primitivas concurrentes propias.

---

## 5. Plan de implementación por fases

# Fase 0 — Baseline crudo (sin refactor)

**Objetivo:** ver herramientas atrapar problemas con el código actual.

## Tareas
1. Añadir nightly toolchain auxiliar para CI.
2. Instalar herramientas:
   - `cargo-careful`
   - `cargo-geiger`
   - `cargo-fuzz`
   - Miri
   - Kani
3. Correr sobre el estado actual:
   - `cargo +nightly careful test`
   - ASan
   - MSan
   - `cargo +nightly miri test` sobre tests seleccionados
   - `cargo kani` sobre 2-3 helpers que ya existan
   - `cargo geiger`

## Entregables
- documento `doc/safety/baseline.md`
- tabla de fallos por herramienta
- lista de archivos/rutas rotas
- primer inventario de `unsafe`

## Meta
**No corregir aún todo**. Solo obtener señal real y priorizar.

---

# Fase 1 — Contratos y refactor mínimo

**Objetivo:** hacer el código verificable sin reescribir los kernels.

## Tareas
1. Crear `src/engine/common/*`.
2. Mover allí:
   - pointer math
   - safe ends
   - guards de reads/writes
   - overstore slack
3. Reemplazar guards dispersos por helpers comunes.
4. Añadir wrappers checked para loads/stores.
5. Añadir comentarios `// SAFETY:` y activar lints de `unsafe`.

## Entregables
- reducción de aritmética cruda dispersa
- contratos explícitos por helper
- código listo para Miri/Kani

## Meta
Toda operación peligrosa debe poder responder:
- qué precondición requiere
- dónde se comprueba
- qué helper la modela

---

# Fase 2 — Suite de contratos y tests de frontera

**Objetivo:** construir una suite pequeña pero letal para bordes.

## Tareas
1. Crear `tests/contracts_*`.
2. Añadir tests paramétricos para:
   - inputs 0..128
   - `out` offsets 0..15
   - padding válido e inválido
   - tails 1/2/3
   - cases strict vs non-strict
3. Añadir macros o funciones auxiliares de differential testing.

## Entregables
- `tests/contracts_ptr_math.rs`
- `tests/contracts_tails.rs`
- `tests/contracts_overstore.rs`
- `tests/contracts_semantics.rs`

## Meta
Aumentar señal para:
- `cargo-careful`
- ASan/MSan
- Miri

---

# Fase 3 — `cargo-careful` verde

**Objetivo:** que la red barata quede estable y frecuente.

## Tareas
1. Correr `cargo +nightly careful test`.
2. Corregir todos los fallos.
3. Integrarlo a CI por PR.

## Entregables
- workflow `careful.yml`
- target en `Justfile`

## Meta
Cualquier regresión obvia de UB/alignment/invariantes debe morir aquí.

---

# Fase 4 — ASan verde

**Objetivo:** estabilizar memoria cruda y overstores.

## Tareas
1. Configurar job ASan Linux x86_64.
2. Correr suite completa.
3. Corregir:
   - OOB
   - slack mal calculado
   - preload guards insuficientes
   - encode overlap bugs
4. Añadir tests específicos de subslice/desalineación.

## Entregables
- workflow `asan.yml`
- guía `doc/safety/asan.md`

## Meta
ASan en verde sobre tests y, si es viable, sobre fuzz smoke targets cortos.

---

# Fase 5 — MSan útil y estable

**Objetivo:** eliminar lecturas no inicializadas.

## Tareas
1. Configurar job MSan Linux x86_64.
2. Priorizar tests de:
   - scalar
   - tails
   - wrappers checked
   - differential small-input
3. Corregir cualquier uso de bytes/lane basura.
4. Añadir un caso mínimo que valide que MSan está “vivo” y no mal configurado.

## Entregables
- workflow `msan.yml`
- guía `doc/safety/msan.md`

## Meta
MSan debe dar señal útil, no spam. Si empieza a producir ruido, revisar instrumentación total antes de culparlo por deporte.

---

# Fase 6 — Miri quirúrgico

**Objetivo:** blindar contracts y helpers.

## Tareas
1. Crear suite `tests/miri/*`.
2. Marcar tests incompatibles con `#[cfg_attr(miri, ignore)]`.
3. Ejecutar:
   - perfil base
   - perfil seeds
   - big-endian
4. Corregir:
   - alignment
   - provenance
   - arithmetic fuera de objeto
   - aliasing accidental

## Entregables
- workflow `miri.yml`
- `doc/safety/miri.md`
- lista de tests soportados por Miri

## Meta
Miri debe cubrir toda la periferia peligrosa del SIMD.

---

# Fase 7 — Kani sobre contracts

**Objetivo:** prueba exhaustiva de invariantes pequeños.

## Tareas
1. Crear harnesses mínimos.
2. Usar `cargo kani --harness ...` por grupos.
3. Añadir bounds de unwind donde haga falta.
4. Mantener el alcance chico y bien modelado.

## Entregables
- módulo `verify/`
- workflow `kani.yml`
- `doc/safety/kani.md`

## Meta
Que los helpers fundamentales queden demostrados, no “probados de casualidad”.

---

# Fase 8 — Fuzzing industrial

**Objetivo:** differential testing masivo.

## Tareas
1. Inicializar `cargo-fuzz`.
2. Crear targets:
   - `decode_diff`
   - `encode_diff`
   - `roundtrip`
   - `invalid_semantics`
   - `tail_alignment`
3. Añadir corpus semilla:
   - tamaños 0..128
   - casos frontera de padding
   - inputs inválidos
   - subslices desalineadas
4. Correr fuzzing corto en CI y largo en jobs programados.

## Entregables
- carpeta `fuzz/`
- corpus semilla versionado
- workflow `fuzz-smoke.yml`
- workflow `fuzz-nightly.yml`

## Meta
Que cualquier backend que diverja del scalar sea cazado rápidamente.

---

# Fase 9 — Gobernanza de seguridad

**Objetivo:** que el stack se mantenga vivo y no se convierta en decoración ritual.

## Política de PR
Todo PR que toque `unsafe`, intrinsics, o guards de memoria debe incluir:

1. explicación del cambio
2. contrato de seguridad actualizado
3. tests añadidos/ajustados
4. impacto sobre:
   - Miri
   - Kani
   - fuzzing
   - sanitizers

## Política de releases
Antes de publicar:
- `cargo geiger`
- `cargo +nightly careful test`
- ASan
- MSan
- Miri suite
- Kani critical harnesses
- fuzz smoke
- differential full test suite en hardware disponible

---

## 6. Plan de CI concreto

# 6.1. Jobs mínimos por PR

1. **stable unit**
   - `cargo test --lib --tests --doc`

2. **nightly careful**
   - `cargo +nightly careful test`

3. **asan**
   - Linux x86_64
   - `-Zbuild-std`

4. **geiger**
   - reportar diff del uso de `unsafe`

5. **fuzz smoke**
   - 1-3 minutos por target esencial

# 6.2. Jobs nightly / scheduled

1. **msan**
2. **miri-base**
3. **miri-many-seeds**
4. **miri-big-endian**
5. **kani-critical**
6. **kani-extended**
7. **fuzz-long**
8. **randomize-layout**

# 6.3. Jobs por hardware/arquitectura específica

## x86_64 común
- scalar
- SSSE3
- AVX2
- WASM host-side tests

## AVX512
- self-hosted runner o cloud runner dedicado
- tests + differential encode/decode
- preferible ASan smoke también

## aarch64
- NEON tests
- ASan/MSan donde soporte y runner lo permitan

## wasm32
- correctness diferencial y property tests
- Miri no será el protagonista aquí; el valor está más en differential/fuzz

---

## 7. Matriz de verificación por clase de bug

| Clase de bug | careful | ASan | MSan | Miri | Kani | fuzz |
|---|---:|---:|---:|---:|---:|---:|
| OOB read/write | medio | alto | bajo | alto | medio | alto* |
| use-after-free | bajo | alto | bajo | alto | bajo | medio* |
| uninitialized read | bajo | bajo | **alto** | alto | bajo | alto* |
| alignment | medio | medio | bajo | **alto** | medio | bajo |
| pointer outside object | bajo | medio | bajo | **alto** | **alto** | medio* |
| overflow en guards | bajo | bajo | bajo | medio | **alto** | medio |
| aliasing/provenance | bajo | bajo | bajo | **alto** | bajo | bajo |
| divergencia funcional | bajo | bajo | bajo | bajo | medio | **alto** |
| data races | bajo | bajo | bajo | medio | bajo | bajo |

`*` cuando el fuzzer corre junto con sanitizer/oráculo fuerte.

---

## 8. Plan de refactor por backend

# 8.1. `scalar.rs`
## Prioridad
Muy alta. Es el oráculo funcional.

## Objetivos
- blindar `decoded_len_strict`
- blindar `encode_base64_fast`
- blindar decode tails
- usarlo como referencia para fuzzing/differential

## Herramientas clave
- MSan
- Miri
- Kani
- cargo-careful

---

# 8.2. `ssse3.rs`
## Prioridad
Muy alta.

## Riesgos
- preámbulos de alineación
- guards de 16 bytes
- overstores de 12+4 / 48+4
- semántica strict vs non-strict

## Herramientas clave
- ASan
- Miri sobre helpers
- Kani sobre guards/slack
- fuzz diferencial vs scalar

---

# 8.3. `avx2.rs`
## Prioridad
Muy alta.

## Riesgos
- overlap encode path (`ip - 4`)
- stores anchos
- DS128 drains
- múltiples paths (unchecked, partial, strict)

## Herramientas clave
- ASan
- MSan
- Kani sobre `base`, `ob`, slack
- fuzz diferencial

---

# 8.4. `avx512vbmi.rs`
## Prioridad
Alta, pero depende de hardware.

## Riesgos
- stores de 64 bytes con solo parte meaningful
- slack grande
- coverage real limitada sin runner AVX512

## Herramientas clave
- ASan en hardware AVX512
- fuzz diferencial en runner adecuado
- Kani para guards y slack
- Miri solo sobre helpers compartidos, no sobre intrinsics reales

---

# 8.5. `neon.rs`
## Prioridad
Alta.

## Riesgos
- tablas y de-lookup
- loads/stores de 16 bytes
- cobertura limitada si no tienes runner ARM

## Herramientas clave
- differential testing
- ASan/MSan en aarch64 cuando sea posible
- Kani/Miri sobre helpers compartidos

---

# 8.6. `wasm_simd128.rs`
## Prioridad
Media-alta.

## Riesgos
- diferencia entre `relaxed-simd` y fallback
- packing emulado
- semántica WASM vs scalar

## Herramientas clave
- differential tests
- fuzzing
- property tests
- Kani/Miri sobre contratos compartidos, no tanto sobre intrinsics WASM

---

## 9. Artefactos concretos que deberías añadir

## 9.1. Archivos de documentación

```text
doc/
  safety/
    baseline.md
    contracts.md
    careful.md
    asan.md
    msan.md
    miri.md
    kani.md
    fuzzing.md
    ci-matrix.md
```

## 9.2. Tests

```text
tests/
  contracts_ptr_math.rs
  contracts_bounds.rs
  contracts_overstore.rs
  contracts_semantics.rs
  miri_bounds.rs
  miri_ptr_math.rs
  miri_scalar_tails.rs
  backend_diff.rs
```

## 9.3. Fuzz

```text
fuzz/
  fuzz_targets/
    decode_diff.rs
    encode_diff.rs
    roundtrip.rs
    invalid_semantics.rs
    tail_alignment.rs
```

## 9.4. Scripts / Justfile

Targets sugeridos:

```text
just test
just test-careful
just test-asan
just test-msan
just test-miri
just test-miri-seeds
just test-miri-big-endian
just test-kani
just test-kani-critical
just fuzz-smoke
just fuzz-long
just geiger
just safety-all
```

---

## 10. Orden exacto recomendado

Si quieres máximo retorno por esfuerzo, este es el orden correcto:

1. **Fase 0 baseline**
2. **Fase 1 contratos + refactor mínimo**
3. **cargo-careful**
4. **ASan**
5. **MSan**
6. **Miri**
7. **Kani**
8. **cargo-fuzz**
9. **Gobernanza / release gate**

### Por qué este orden

- `cargo-careful` y ASan te dan valor rápido sobre el código actual.
- MSan encuentra otra clase de mugre importante.
- Miri y Kani rinden mucho más cuando ya factorizaste helpers.
- fuzzing explota mejor cuando hay oráculos claros y contratos estables.

---

## 11. Criterios de salida (“done means done”)

El stack estará bien integrado cuando se cumpla todo esto:

### Correctness / safety
- [ ] ningún `ptr.add()` usado como simple guard sin comprobar `remaining`
- [ ] todo load/store SIMD pasa por un wrapper checked o contrato equivalente centralizado
- [ ] todos los overstores tienen helper y test dedicado
- [ ] la semántica strict/non-strict está documentada y testeada

### Herramientas
- [ ] `cargo +nightly careful test` verde
- [ ] ASan verde
- [ ] MSan verde
- [ ] Miri base verde
- [ ] Miri many-seeds verde
- [ ] Miri big-endian verde para tests soportados
- [ ] Kani critical harnesses verdes
- [ ] fuzz smoke verde
- [ ] `cargo geiger` baseline guardado

### Proceso
- [ ] todo nuevo `unsafe` requiere contrato
- [ ] todo cambio de backend requiere differential test
- [ ] release gate incluye safety matrix completa

---

## 12. Resumen ejecutivo

Para que `oxid64` sea un engine “grado industrial”, el stack no debe tratarse como un conjunto de herramientas sueltas, sino como una **arquitectura de verificación**:

- `cargo-careful` detecta problemas baratos y frecuentes
- ASan atrapa memoria cruda y overstores
- MSan atrapa lecturas no inicializadas
- Miri verifica UB semántico fino
- Kani demuestra contratos pequeños
- `cargo-fuzz` caza divergencias y combina brutalmente bien con sanitizers

La clave para sacarles el máximo provecho no es “ejecutarlos más fuerte”, sino **hacer verificable el diseño del engine**:
- helpers pequeños
- contratos explícitos
- wrappers checked
- scalar como oráculo
- differential testing sistemático
- CI por capas

Ese es el camino paranoico correcto. El otro camino es esperar a que un `ptr.add()` mal puesto te enseñe metafísica a las 3 de la mañana.

---

## 13. Referencias oficiales / primarias

- Rust Unstable Book — Sanitizers  
  https://doc.rust-lang.org/beta/unstable-book/compiler-flags/sanitizer.html

- Miri README / documentación del proyecto  
  https://github.com/rust-lang/miri

- Kani usage docs  
  https://model-checking.github.io/kani/usage.html

- Kani repository  
  https://github.com/model-checking/kani

- cargo-careful  
  https://github.com/RalfJung/cargo-careful

- Rust Fuzz Book / cargo-fuzz  
  https://rust-fuzz.github.io/book/cargo-fuzz.html

- cargo-geiger  
  https://github.com/geiger-rs/cargo-geiger

- `unsafe_op_in_unsafe_fn` (Edition Guide)  
  https://doc.rust-lang.org/edition-guide/rust-2024/unsafe-op-in-unsafe-fn.html

- Clippy config for `undocumented_unsafe_blocks`  
  https://doc.rust-lang.org/stable/clippy/lint_configuration.html

- `-Zrandomize-layout`  
  https://doc.rust-lang.org/stable/unstable-book/compiler-flags/randomize-layout.html

- Loom  
  https://github.com/tokio-rs/loom
