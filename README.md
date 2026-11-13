# Bancada

Peças compartilhadas entre o Chess Studio e o Seu Bolso Feliz desktop (ADR-018 do SBF):
shell egui sem Bevy, segredos, rede e armazenamento SQLite. Licença MIT OR Apache-2.0,
identificadores em português.

| Crate | O que é |
|---|---|
| `bancada-armazem` | SQLite: PRAGMAs, escada de migrações por `user_version`, transação curta, tabela só de acréscimo, cópia pela API de backup |
| `bancada-segredos` | Keyring do sistema operacional, com fallback em arquivo cifrado por senha-mestra |
| `bancada-casca` | Ribbon, Ações, Paleta, KeyTips, painel de navegação, dock, barra de status, temas e i18n (a extrair) |
| `bancada-rede` | Portão de rede, cache e OAuth por loopback (a extrair) |

Repositório local: ainda não existe no GitHub.
