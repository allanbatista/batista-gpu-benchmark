# Repository Guidelines

## Estrutura do projeto

`src/main.rs` inicia a CLI e seleciona o adaptador; `src/app.rs` monta a aplicação Bevy. O código é dividido por responsabilidade: `bench/` mede e gera relatórios, `config/` trata presets e argumentos, `scene/` define a carga determinística, `render_cfg/` configura renderização, `ui/` contém painéis e resultados, e `platform/` coleta telemetria. Modelos oficiais ficam em `assets/models/`; preserve também `ATTRIBUTION.md`. `scripts/` gera assets, enquanto `packaging/`, `snap/` e `.github/workflows/` mantêm distribuição e CI. Não versione saídas de `target/`, `results/` ou `config/settings.toml`.

## Build, testes e desenvolvimento

Use Rust estável e execute a partir da raiz:

```bash
cargo run --release            # abre a interface local
make build                     # compila o binário otimizado
make test                      # executa testes no perfil release
cargo fmt --all -- --check     # verifica formatação
cargo clippy --all-targets     # aponta problemas idiomáticos
```

Para validar hardware e backends, use `cargo run --release -- --list-adapters`. Os alvos `make deb`, `make rpm`, `make appimage`, `make snap` e `make flatpak` exigem as ferramentas descritas no `README.md`.

## Estilo e nomenclatura

Siga o `rustfmt` padrão: quatro espaços, imports organizados e sem alinhamento manual. Use `snake_case` para módulos, funções e testes; `PascalCase` para tipos; `SCREAMING_SNAKE_CASE` para constantes. Mantenha lógica de benchmark pura e determinística, valide entradas na fronteira da CLI e evite alocações ou I/O durante a captura de frames. Prefira mudanças pequenas no módulo responsável.

## Diretrizes de testes

Coloque testes unitários em `#[cfg(test)] mod tests` junto ao código, com nomes descritivos como `same_seed_same_rig`. Cubra limites, entradas inválidas e invariantes de métricas ou determinismo. Não há meta numérica de cobertura; toda mudança em comportamento crítico deve incluir uma regressão. Antes do PR, execute `make test` e, quando houver alteração visual ou de GPU, valide manualmente no backend afetado.

## Commits e pull requests

O histórico usa assuntos curtos e descritivos, com escopo opcional, por exemplo `Packaging: ...` ou `F3+F4: ...`. Mantenha cada commit em uma alteração lógica. No PR, descreva o comportamento alterado, informe comandos e plataformas testadas, vincule a issue aplicável e inclua capturas para mudanças visuais. Destaque alterações em presets, métricas ou GLBs: elas podem invalidar a comparabilidade de relatórios e exigir versionamento ou atribuição atualizados.
