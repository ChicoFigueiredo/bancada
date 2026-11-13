//! Erros da guarda, em português. Nenhuma variante carrega segredo, senha, rótulo nem conteúdo
//! de arquivo: o texto pode ir para log e diagnóstico como está.

use std::fmt;

/// Tamanho mínimo da senha-mestra, em caracteres.
pub const TAMANHO_MINIMO_DA_SENHA: usize = 8;

/// Uma falha do keyring do sistema operacional, sem o texto que ele devolveu (os erros do
/// sistema podem citar bytes do segredo; nunca são repassados).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum FalhaDoChaveiro {
    /// Não há keyring nesta sessão (Linux sem Secret Service, sessão sem D-Bus).
    Indisponivel,
    /// O keyring negou acesso ou está trancado.
    AcessoNegado,
    /// O valor ou o rótulo passa do limite do keyring.
    LongoDemais,
    /// O keyring devolveu algo ilegível (não é texto).
    Ilegivel,
    /// Outra falha do keyring.
    Outra,
}

impl FalhaDoChaveiro {
    fn descricao(self) -> &'static str {
        match self {
            Self::Indisponivel => "não há keyring do sistema nesta sessão",
            Self::AcessoNegado => "acesso negado ou keyring trancado",
            Self::LongoDemais => "valor ou rótulo longo demais para o keyring do sistema",
            Self::Ilegivel => "o keyring do sistema devolveu um valor ilegível",
            Self::Outra => "falha no keyring do sistema",
        }
    }
}

/// Por que a guarda não fez o que se pediu.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ErroDeSegredo {
    /// Não há keyring do sistema e o arquivo cifrado ainda não foi criado: nada foi gravado.
    Indisponivel,
    /// O arquivo cifrado está trancado: destranque com a senha-mestra.
    Trancado,
    /// A operação é do arquivo cifrado, e a guarda usa o keyring do sistema (ou nada).
    SemArquivoCifrado,
    /// O arquivo cifrado só é criado quando não há keyring do sistema e ainda não existe um.
    NaoPodeCriarArquivo,
    /// A senha-mestra está errada **ou** o arquivo foi alterado, cortado ou corrompido. As
    /// duas causas dão o mesmo erro, de propósito: nada é lido do arquivo.
    SenhaOuArquivoInvalido,
    /// Arquivo (cifrado ou índice) de uma versão mais nova da bancada.
    FormatoNaoSuportado { versao: u16 },
    /// Senha-mestra curta demais.
    SenhaFraca { minimo: usize },
    /// Custo do Argon2id abaixo do mínimo seguro ou acima do que a leitura aceita.
    ParametrosInvalidos,
    /// Segredo vazio.
    SegredoVazio,
    /// Rótulo vazio, longo demais ou com caractere de controle.
    RotuloInvalido,
    /// Não há segredo guardado com esse rótulo.
    NaoEncontrado,
    /// O índice de rótulos está ilegível ou foi alterado.
    IndiceCorrompido,
    /// O keyring do sistema falhou.
    ChaveiroDoSistema(FalhaDoChaveiro),
    /// Falha de disco no diretório da guarda.
    Disco { tipo: std::io::ErrorKind },
    /// Falha interna (gerador aleatório do sistema, cifra), com mensagem fixa.
    Interno { detalhe: &'static str },
}

impl ErroDeSegredo {
    pub(crate) fn disco(erro: &std::io::Error) -> Self {
        Self::Disco { tipo: erro.kind() }
    }
}

impl From<FalhaDoChaveiro> for ErroDeSegredo {
    fn from(falha: FalhaDoChaveiro) -> Self {
        Self::ChaveiroDoSistema(falha)
    }
}

impl fmt::Display for ErroDeSegredo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Indisponivel => f.write_str(
                "Não há keyring do sistema disponível: crie o arquivo de segredos, protegido \
                 por senha-mestra. Nada foi gravado.",
            ),
            Self::Trancado => {
                f.write_str("O arquivo de segredos está trancado: destranque-o com a senha-mestra.")
            }
            Self::SemArquivoCifrado => f.write_str(
                "Os segredos estão no keyring do sistema: não há senha-mestra para isto.",
            ),
            Self::NaoPodeCriarArquivo => f.write_str(
                "O arquivo de segredos só é criado quando o sistema não oferece keyring e ainda \
                 não existe um.",
            ),
            Self::SenhaOuArquivoInvalido => f.write_str(
                "Não foi possível abrir o arquivo de segredos: a senha-mestra está errada ou o \
                 arquivo foi alterado. Nada foi lido dele.",
            ),
            Self::FormatoNaoSuportado { versao } => write!(
                f,
                "O arquivo de segredos é de uma versão mais nova (formato {versao})."
            ),
            Self::SenhaFraca { minimo } => write!(
                f,
                "A senha-mestra precisa ter pelo menos {minimo} caracteres."
            ),
            Self::ParametrosInvalidos => {
                f.write_str("Parâmetros do Argon2id fora dos limites aceitos.")
            }
            Self::SegredoVazio => f.write_str("O segredo está vazio."),
            Self::RotuloInvalido => {
                f.write_str("Rótulo inválido: vazio, longo demais ou com caractere de controle.")
            }
            Self::NaoEncontrado => f.write_str("Não há segredo guardado com esse rótulo."),
            Self::IndiceCorrompido => f.write_str(
                "O índice de rótulos foi alterado ou está corrompido; nada foi lido dele.",
            ),
            Self::ChaveiroDoSistema(falha) => {
                write!(f, "Keyring do sistema: {}.", falha.descricao())
            }
            Self::Disco { tipo } => write!(f, "Falha de disco na guarda de segredos: {tipo}."),
            Self::Interno { detalhe } => {
                write!(f, "Falha interna da guarda de segredos: {detalhe}.")
            }
        }
    }
}

impl std::error::Error for ErroDeSegredo {}
