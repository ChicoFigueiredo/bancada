//! Escada de migrações numerada por `PRAGMA user_version`.
//!
//! - Cada migração roda na própria transação IMMEDIATE (o DDL do SQLite é transacional), então
//!   uma falha no meio deixa o banco na versão anterior.
//! - A versão é relida dentro da transação: dois processos migrando o mesmo arquivo aplicam
//!   cada migração uma vez só.
//! - Banco mais novo que o app é recusado ([`Erro::BancoMaisNovo`]), sem tocar nele.
//! - Uma [`LinhaDeBase`] opcional adota um banco criado por outro sistema.
//!
//! Regra de ouro: migração já publicada nunca é editada; só se acrescenta uma nova.

use crate::erro::{Erro, Resultado};
use rusqlite::{Connection, OptionalExtension, Transaction, TransactionBehavior};

type Codigo = fn(&Transaction<'_>) -> rusqlite::Result<()>;

#[derive(Clone, Copy)]
enum Acao {
    Sql(&'static str),
    Codigo(Codigo),
}

impl std::fmt::Debug for Acao {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Sql(_) => f.write_str("Sql"),
            Self::Codigo(_) => f.write_str("Codigo"),
        }
    }
}

/// Um degrau da escada: o que leva o banco da versão `n - 1` à versão `n`, onde `n` é a
/// posição (a partir de 1) na lista passada a [`Escada::nova`].
#[derive(Debug, Clone, Copy)]
pub struct Migracao {
    /// Texto curto que descreve a mudança (aparece nos erros e na lista de pendentes).
    pub descricao: &'static str,
    acao: Acao,
}

impl Migracao {
    /// Migração em SQL (um ou vários comandos).
    #[must_use]
    pub const fn sql(descricao: &'static str, sql: &'static str) -> Self {
        Self {
            descricao,
            acao: Acao::Sql(sql),
        }
    }

    /// Migração em código, para o que o SQL puro não alcança (SQL gerado, cópia de dados...).
    /// Roda dentro da mesma transação da migração; devolver erro desfaz tudo.
    #[must_use]
    pub const fn codigo(descricao: &'static str, codigo: Codigo) -> Self {
        Self {
            descricao,
            acao: Acao::Codigo(codigo),
        }
    }

    fn aplicar(&self, tx: &Transaction<'_>) -> rusqlite::Result<()> {
        match self.acao {
            Acao::Sql(sql) => tx.execute_batch(sql),
            Acao::Codigo(f) => f(tx),
        }
    }
}

/// Confere se a tabela `meta`-estilo de um banco de outro sistema diz o que se espera:
/// `consulta` deve devolver uma linha com uma coluna de texto igual a `esperado`.
#[derive(Debug, Clone, Copy)]
pub struct MarcadorDeOrigem<'a> {
    /// SELECT que devolve o valor do marcador (a tabela dele deve constar em
    /// [`LinhaDeBase::tabelas`]).
    pub consulta: &'a str,
    /// Valor esperado.
    pub esperado: &'a str,
}

/// Adoção de um banco criado por outro sistema (por exemplo, o `inventario.sqlite` do Bun).
///
/// Quando o banco tem tabelas, `user_version = 0` e a escada traz uma linha de base, o armazém
/// confere que as `tabelas` existem (e o marcador, se houver) e **só então** carimba
/// `user_version = versao`, sem rodar as migrações até ela. Banco vazio não é adotado: roda a
/// escada inteira, e a migração `versao` deve criar o mesmo esquema.
#[derive(Debug, Clone, Copy)]
pub struct LinhaDeBase<'a> {
    /// Versão que o carimbo grava (a migração equivalente ao esquema adotado).
    pub versao: u32,
    /// Tabelas que precisam existir.
    pub tabelas: &'a [&'a str],
    /// Verificação extra de origem, opcional.
    pub marcador: Option<MarcadorDeOrigem<'a>>,
}

impl<'a> LinhaDeBase<'a> {
    /// Linha de base que exige as `tabelas` e carimba `versao`.
    #[must_use]
    pub const fn nova(versao: u32, tabelas: &'a [&'a str]) -> Self {
        Self {
            versao,
            tabelas,
            marcador: None,
        }
    }

    /// Exige também que `consulta` devolva `esperado`.
    #[must_use]
    pub const fn com_marcador(mut self, consulta: &'a str, esperado: &'a str) -> Self {
        self.marcador = Some(MarcadorDeOrigem { consulta, esperado });
        self
    }
}

/// A lista ordenada de migrações de um app, com a linha de base opcional.
#[derive(Debug, Clone, Copy)]
pub struct Escada<'a> {
    migracoes: &'a [Migracao],
    linha_de_base: Option<LinhaDeBase<'a>>,
}

impl<'a> Escada<'a> {
    /// A migração de índice `i` produz a versão `i + 1`.
    #[must_use]
    pub const fn nova(migracoes: &'a [Migracao]) -> Self {
        Self {
            migracoes,
            linha_de_base: None,
        }
    }

    /// Acrescenta a linha de base (adoção de banco de outro sistema).
    #[must_use]
    pub const fn com_linha_de_base(mut self, linha: LinhaDeBase<'a>) -> Self {
        self.linha_de_base = Some(linha);
        self
    }

    /// Versão de esquema desta build (quantidade de migrações).
    #[must_use]
    #[allow(clippy::cast_possible_truncation)]
    pub const fn versao_mais_recente(&self) -> u32 {
        self.migracoes.len() as u32
    }
}

/// O que [`migrar`] fez.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RelatorioMigracao {
    /// `user_version` ao começar.
    pub versao_inicial: u32,
    /// `user_version` ao terminar.
    pub versao_final: u32,
    /// `true` se um banco de outro sistema foi adotado pela linha de base.
    pub linha_de_base_carimbada: bool,
}

/// `PRAGMA user_version` da conexão.
///
/// # Errors
/// Falha do SQLite.
pub fn versao_do_banco(conexao: &Connection) -> Resultado<u32> {
    Ok(conexao.query_row("PRAGMA user_version", [], |l| l.get(0))?)
}

fn tem_tabelas_de_usuario(conexao: &Connection) -> Resultado<bool> {
    Ok(conexao.query_row(
        "SELECT EXISTS (SELECT 1 FROM sqlite_master
                        WHERE type = 'table' AND name NOT LIKE 'sqlite\\_%' ESCAPE '\\')",
        [],
        |l| l.get(0),
    )?)
}

fn validar_escada(escada: &Escada<'_>) -> Resultado<()> {
    if let Some(base) = &escada.linha_de_base
        && (base.versao == 0 || base.versao > escada.versao_mais_recente())
    {
        return Err(Erro::EscadaInconsistente(format!(
            "a linha de base carimba a versão {}, fora de 1..={}",
            base.versao,
            escada.versao_mais_recente()
        )));
    }
    Ok(())
}

/// Versão a partir da qual as migrações valem, sem alterar nada: a do banco, ou a da linha de
/// base se o banco é de outro sistema e ainda vai ser adotado.
fn ponto_de_partida(conexao: &Connection, escada: &Escada<'_>) -> Resultado<u32> {
    validar_escada(escada)?;
    let encontrada = versao_do_banco(conexao)?;
    if encontrada > escada.versao_mais_recente() {
        return Err(Erro::BancoMaisNovo {
            encontrada,
            suportada: escada.versao_mais_recente(),
        });
    }
    if encontrada == 0
        && let Some(base) = &escada.linha_de_base
        && tem_tabelas_de_usuario(conexao)?
    {
        return Ok(base.versao);
    }
    Ok(encontrada)
}

/// As migrações que ainda vão rodar, para mostrar ao usuário o que vai mudar antes de migrar.
/// Num banco de outro sistema a adotar, a linha de base já conta como aplicada (a conferência
/// em si só acontece em [`migrar`]).
///
/// # Errors
/// [`Erro::BancoMaisNovo`], [`Erro::EscadaInconsistente`] ou falha do SQLite.
pub fn pendentes<'a>(conexao: &Connection, escada: &Escada<'a>) -> Resultado<&'a [Migracao]> {
    let inicio = ponto_de_partida(conexao, escada)?;
    Ok(&escada.migracoes[inicio as usize..])
}

/// Confere a linha de base e carimba, tudo numa transação IMMEDIATE. Devolve `true` se
/// carimbou (e `false` se outro processo chegou antes).
fn adotar(conexao: &mut Connection, base: &LinhaDeBase<'_>) -> Resultado<bool> {
    let tx = conexao.transaction_with_behavior(TransactionBehavior::Immediate)?;
    if versao_do_banco(&tx)? != 0 {
        return Ok(false);
    }
    let mut faltando = Vec::new();
    for tabela in base.tabelas {
        let existe: bool = tx.query_row(
            "SELECT EXISTS (SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1)",
            [tabela],
            |l| l.get(0),
        )?;
        if !existe {
            faltando.push((*tabela).to_owned());
        }
    }
    if !faltando.is_empty() {
        return Err(Erro::LinhaDeBaseSemTabelas { faltando });
    }
    if let Some(marcador) = &base.marcador {
        let lido: Option<String> = tx
            .query_row(marcador.consulta, [], |l| l.get(0))
            .optional()?;
        if lido.as_deref() != Some(marcador.esperado) {
            return Err(Erro::LinhaDeBaseMarcadorDiferente {
                esperado: marcador.esperado.to_owned(),
                encontrado: lido,
            });
        }
    }
    tx.pragma_update(None, "user_version", base.versao)?;
    tx.commit()?;
    Ok(true)
}

/// Leva o banco até `alvo` (no máximo a última migração da escada).
///
/// # Errors
/// [`Erro::BancoMaisNovo`], falhas da linha de base, [`Erro::MigracaoFalhou`] (a migração que
/// falhou foi desfeita; as anteriores permanecem) ou falha do SQLite.
pub fn migrar_ate(
    conexao: &mut Connection,
    escada: &Escada<'_>,
    alvo: u32,
) -> Resultado<RelatorioMigracao> {
    validar_escada(escada)?;
    let ultima = escada.versao_mais_recente();
    let versao_inicial = versao_do_banco(conexao)?;
    if versao_inicial > ultima {
        return Err(Erro::BancoMaisNovo {
            encontrada: versao_inicial,
            suportada: ultima,
        });
    }

    let mut linha_de_base_carimbada = false;
    if versao_inicial == 0
        && let Some(base) = &escada.linha_de_base
        && tem_tabelas_de_usuario(conexao)?
    {
        linha_de_base_carimbada = adotar(conexao, base)?;
    }

    let atual = versao_do_banco(conexao)?;
    for versao in atual + 1..=alvo.min(ultima) {
        let migracao = &escada.migracoes[versao as usize - 1];
        let tx = conexao.transaction_with_behavior(TransactionBehavior::Immediate)?;
        // Relida sob o lock de escrita: outro processo pode ter aplicado esta migração.
        let na_transacao = versao_do_banco(&tx)?;
        if na_transacao > ultima {
            return Err(Erro::BancoMaisNovo {
                encontrada: na_transacao,
                suportada: ultima,
            });
        }
        if na_transacao >= versao {
            continue;
        }
        let falhou = |origem| Erro::MigracaoFalhou {
            versao,
            descricao: migracao.descricao,
            origem,
        };
        migracao.aplicar(&tx).map_err(falhou)?;
        tx.pragma_update(None, "user_version", versao)
            .map_err(falhou)?;
        tx.commit().map_err(falhou)?;
    }

    Ok(RelatorioMigracao {
        versao_inicial,
        versao_final: versao_do_banco(conexao)?,
        linha_de_base_carimbada,
    })
}

/// Leva o banco à última versão da escada.
///
/// # Errors
/// Veja [`migrar_ate`].
pub fn migrar(conexao: &mut Connection, escada: &Escada<'_>) -> Resultado<RelatorioMigracao> {
    migrar_ate(conexao, escada, escada.versao_mais_recente())
}
