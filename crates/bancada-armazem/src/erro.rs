//! Erros tipados do armazém.

use std::fmt;
use std::path::PathBuf;

/// Tudo o que pode dar errado ao usar o armazém.
#[derive(Debug)]
#[non_exhaustive]
pub enum Erro {
    /// Falha do SQLite (E/S, disco cheio, banco travado além da espera...).
    Sqlite(rusqlite::Error),
    /// JSON de preferência ilegível, ou valor que não vira JSON.
    Json(serde_json::Error),
    /// Falha de arquivo ao copiar o banco.
    Io(std::io::Error),
    /// O banco foi criado por uma versão mais nova do app: recusado, nada é gravado.
    BancoMaisNovo {
        /// Versão (`user_version`) que está no banco.
        encontrada: u32,
        /// Maior versão que esta build conhece.
        suportada: u32,
    },
    /// Banco de outro sistema sem todas as tabelas esperadas pela linha de base: não é carimbado.
    LinhaDeBaseSemTabelas {
        /// Tabelas esperadas que não existem.
        faltando: Vec<String>,
    },
    /// O marcador de origem do banco de outro sistema não tem o valor esperado.
    LinhaDeBaseMarcadorDiferente {
        /// Valor esperado.
        esperado: String,
        /// Valor lido (`None` se a consulta não devolveu linha).
        encontrado: Option<String>,
    },
    /// A escada está mal montada (por exemplo, linha de base além da última migração).
    EscadaInconsistente(String),
    /// Uma migração falhou e foi desfeita; o banco ficou na versão anterior.
    MigracaoFalhou {
        /// Versão que a migração produziria.
        versao: u32,
        /// Descrição da migração.
        descricao: &'static str,
        /// Falha original do SQLite.
        origem: rusqlite::Error,
    },
    /// A operação só existe para banco em arquivo (banco em memória não tem conexão de leitura
    /// separada nem caminho).
    SemArquivo,
    /// O destino da cópia já existe; a cópia nunca sobrescreve.
    DestinoJaExiste(PathBuf),
    /// `PRAGMA integrity_check` devolveu algo além de `ok`.
    IntegridadeComFalhas(Vec<String>),
    /// Nome de tabela que não é um identificador simples (`[A-Za-z_][A-Za-z0-9_]*`).
    NomeInvalido(String),
}

impl fmt::Display for Erro {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Sqlite(e) => write!(f, "erro do SQLite: {e}"),
            Self::Json(e) => write!(f, "JSON inválido: {e}"),
            Self::Io(e) => write!(f, "falha de arquivo: {e}"),
            Self::BancoMaisNovo {
                encontrada,
                suportada,
            } => write!(
                f,
                "banco na versão de esquema {encontrada}, mas este app só entende até {suportada}; atualize o app"
            ),
            Self::LinhaDeBaseSemTabelas { faltando } => write!(
                f,
                "o banco não bate com a linha de base: faltam as tabelas {}",
                faltando.join(", ")
            ),
            Self::LinhaDeBaseMarcadorDiferente {
                esperado,
                encontrado,
            } => match encontrado {
                Some(v) => write!(
                    f,
                    "o banco não bate com a linha de base: marcador `{v}`, esperado `{esperado}`"
                ),
                None => write!(
                    f,
                    "o banco não bate com a linha de base: marcador ausente, esperado `{esperado}`"
                ),
            },
            Self::EscadaInconsistente(detalhe) => {
                write!(f, "escada de migrações inválida: {detalhe}")
            }
            Self::MigracaoFalhou {
                versao,
                descricao,
                origem,
            } => write!(
                f,
                "a migração {versao} ({descricao}) falhou e foi desfeita: {origem}"
            ),
            Self::SemArquivo => write!(f, "operação disponível só para banco em arquivo"),
            Self::DestinoJaExiste(p) => write!(f, "o destino {} já existe", p.display()),
            Self::IntegridadeComFalhas(linhas) => {
                write!(
                    f,
                    "verificação de integridade falhou: {}",
                    linhas.join("; ")
                )
            }
            Self::NomeInvalido(nome) => write!(f, "`{nome}` não é um nome de tabela válido"),
        }
    }
}

impl std::error::Error for Erro {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Sqlite(e) => Some(e),
            Self::Json(e) => Some(e),
            Self::Io(e) => Some(e),
            Self::MigracaoFalhou { origem, .. } => Some(origem),
            _ => None,
        }
    }
}

impl From<rusqlite::Error> for Erro {
    fn from(e: rusqlite::Error) -> Self {
        Self::Sqlite(e)
    }
}

impl From<serde_json::Error> for Erro {
    fn from(e: serde_json::Error) -> Self {
        Self::Json(e)
    }
}

impl From<std::io::Error> for Erro {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

/// `Result` com [`Erro`].
pub type Resultado<T> = std::result::Result<T, Erro>;
