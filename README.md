# Bancada

Peças compartilhadas entre o Chess Studio e o Seu Bolso Feliz desktop (ADR-018 do SBF):
shell egui sem Bevy, segredos, rede e armazenamento SQLite. Licença MIT OR Apache-2.0,
identificadores em português.

| Crate | O que é |
|---|---|
| `bancada-armazem` | SQLite: PRAGMAs, escada de migrações por `user_version`, transação curta, tabela só de acréscimo, cópia pela API de backup |
| `bancada-segredos` | Keyring do sistema operacional, com fallback em arquivo cifrado por senha-mestra |
| `bancada-casca` | Shell eframe: Ribbon, Ações, Paleta, KeyTips, navegação relacional (histórico, trilha, link de item, painéis flutuantes), dock, barra de status, temas por tokens, i18n Fluent e ícones plugáveis (ver `crates/bancada-casca/README.md`) |
| `bancada-rede` | Portão de rede, cache e OAuth por loopback (a extrair) |

Repositório local: ainda não existe no GitHub.
