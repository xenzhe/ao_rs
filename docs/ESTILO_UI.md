# Numic AO — Guía de estilo de interfaz

Numic AO es un Argentum Online de rol. La interfaz tiene que sentirse **parte del mundo**
(objetos, materiales, luz), no una aplicación de escritorio encima del juego. Esta guía es
obligatoria para cualquier pantalla, ventana o HUD. La base de código está en
`crates/client/src/ui/{theme,widgets,frame,fx_ui}.rs`: úsala, no dibujes rectángulos sueltos.

## 1. Las reglas anti-formulario (lo que NO queremos)

1. **Nada de «caja con etiquetas y campos».** Si una pantalla es una columna de
   `etiqueta: valor` y botones abajo, está mal. Busca la metáfora: un libro de hechizos se
   lee como un libro, un inventario es una mochila/cofre, el personaje es una ficha con
   retrato, la creación es un ritual en el mundo.
2. **Una sola cosa protagonista por pantalla.** Decide qué es lo más importante (el
   personaje, el objeto elegido, la vida) y dale tamaño, luz y contraste. Todo lo demás
   baja un nivel.
3. **Jerarquía en tres niveles, siempre:**
   - **N1 — Escena / título**: tipografía Display o Title grande, oro con luz, mucho aire.
   - **N2 — Secciones**: Title pequeño en versalitas, separador ornamental, sin caja.
   - **N3 — Datos**: Body, color atenuado para etiquetas, color pleno para valores.
   Si todo tiene borde dorado, nada destaca: el borde ornamental es **solo** para el marco
   de ventana (N1). Dentro, separadores y fondos sutiles, no cajas.
4. **Nada de bordes de 1 px por todas partes.** Agrupa con espacio, fondo y alineación.
5. **Números con intención.** Las cifras clave (vida, oro, nivel, precio) van grandes,
   con su icono o color propio; las secundarias, pequeñas. Nunca una tabla de números
   iguales.
6. **Cada acción responde** (regla de `experiencia-jugador`): hover visible, pulsado que
   se hunde, sonido/destello al confirmar, error en humano junto a donde ocurrió (no solo
   en la consola).
7. **Movimiento con sentido, nunca decorativo de más.** Abrir ventana: 140 ms de fundido
   + escala 0,97→1. Hover: 80 ms. Avisos: entran y se van solos. Nada que parpadee sin
   motivo. Todo respeta el rendimiento (§6).
8. **El mundo detrás importa.** Las pantallas de cuenta, selección y creación son escenas
   del juego (mapa real, luz, personajes caminando), no fondos planos.
9. **Texto humano y de rol.** «Tu bolsa está llena» y no «Inventario lleno (42/42)».
   Los títulos pueden tener voz («Forja tu destino»), los datos son claros.
10. **Consistencia antes que creatividad.** Mismo botón, mismo marco, misma separación en
    todas las ventanas. La personalidad la ponen la escena, las fuentes y los materiales.

## 2. Tipografía (`theme::FontKind`)

| Uso | Fuente | Cuándo |
|---|---|---|
| `Display` | Cinzel Decorative Bold | Logo, títulos de escena (≥ 40 px). Una vez por pantalla. |
| `Title` | Cinzel | Títulos de ventana, secciones (en versalitas), nombres importantes. |
| `Narrative` / `NarrativeItalic` | IM Fell English | Descripciones de rol, citas, lore, tooltips de objeto (la frase). |
| `Body` | Alegreya Sans AO | Datos, botones, listas, números. La fuente de trabajo. |
| `Script` | Almendra | Firmas, sellos, detalles ornamentales (poco). |

Escala (multiplicada por `u`): 12 · 14 · 16 · 18 · 22 · 28 · 36 · 48 · 64. No inventes
tamaños intermedios (el atlas de glifos lo agradece, ver `widgets::glyph`).

## 3. Color (`theme`)

- **Fondos**: `INK_DEEP` (casi negro cálido), `LEATHER` (cuero oscuro), `WOOD`.
- **Superficies**: `PANEL` (fondo de ventana con textura), `RECESS` (hundido: casillas,
  campos), `RAISED` (elevado: botones secundarios).
- **Acento**: `GOLD` (marcos, iconos), `GOLD_BRIGHT` (texto N1, hover), `GOLD_DIM`
  (separadores). El oro es escaso: si hay más de ~10 % de oro en una ventana, sobra.
- **Texto**: `TEXT` (valores), `TEXT_DIM` (etiquetas), `TEXT_FAINT` (deshabilitado).
- **Semánticos**: `HP` rojo sangre, `MANA` azul profundo, `STAMINA` verde musgo,
  `HUNGER` ámbar, `THIRST` agua, `GOOD`, `BAD`, `WARN`. Rarezas de objeto si existen.
- Contraste mínimo legible: texto sobre fondo ≥ 4.5:1 para Body.

## 4. Materiales y marcos (`theme::Frame`)

- **Ventana (N1)**: marco de metal dorado con esquinas ornamentales, placa de título
  centrada que sobresale del borde superior, fondo de cuero/pergamino oscuro con textura
  procedural, sombra suave debajo, viñeta interior.
- **Sección (N2)**: sin caja. Título en versalitas + separador `—◆—`.
- **Hundido (casillas, campos, listas)**: fondo `RECESS` con sombra interior arriba.
- **Pergamino** (descripciones, misiones, cartas): textura clara con texto `INK`.
- **Botones**: `Primary` (oro grabado, uno por pantalla), `Secondary` (cuero con borde
  fino), `Ghost` (solo texto, subrayado al hover), `Danger` (rojo apagado).

## 5. Composición

- Rejilla de 8 px × `u`. Márgenes de ventana 20 px, entre secciones 16-24 px.
- Alineación por columnas; etiquetas a la izquierda atenuadas, valores alineados.
- Iconos antes que texto donde el icono se entiende (oro, vida, peso, nivel).
- Tooltips ricos: nombre en el color de su tipo, frase de rol en `NarrativeItalic`,
  datos en Body, requisitos en rojo si no se cumplen.

## 6. Rendimiento

- Nada de texturas nuevas por fotograma: se generan una vez (`theme` las cachea).
- Pocos tamaños de fuente (escala de §2).
- Animaciones con estado por id (`fx_ui`), sin asignaciones por fotograma en bucles calientes.
- Objetivo: el HUD completo + 3 ventanas abiertas a 1920×1080 no baja más de un 10 % los fps
  respecto a sin UI.

## 7. Cómo se revisa

Captura a 1280×800 y 1920×1080 (`AO_SHOT`, `AO_OPEN`, ver `docs/P3_CONTRATO.md`) y se
comprueba: ¿se entiende qué es lo importante en 1 segundo? ¿hay alguna caja que sobre?
¿todo el texto se lee? ¿se siente del mundo de AO y no de una web? Si alguna respuesta es
no, no está terminado.
