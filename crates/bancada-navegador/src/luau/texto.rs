//! A biblioteca `string` do roteiro, com as funções de busca por padrão trocadas por
//! versões que o host controla.
//!
//! O `interrupt` do Luau só roda entre instruções do VM; uma única chamada a
//! `string.find`/`match`/`gmatch`/`gsub` executa em C sem nunca devolver o controle. Padrões
//! como `%b()` (O(n²)) ou com retroreferência `(.-)%1b` (O(n²) ou pior) travariam a thread
//! por minutos, sem que o teto de tempo nem o cancelamento surtissem efeito. Por isso estas
//! funções recusam, antes de chamar a original:
//!
//! - padrões com `%b` e com retroreferência `%1`..`%9`;
//! - texto de busca acima de [`SUJEITO_MAXIMO`] e padrão acima de [`PADRAO_MAXIMO`];
//! - em `string.rep`, resultado acima de [`REPETICAO_MAXIMA`].
//!
//! A busca literal (`find(s, p, init, true)`) continua permitida: não interpreta padrão.

use mlua::{Error, Function, Lua, MultiValue, Result, Table, Value, Variadic};

/// Tamanho máximo do texto onde se busca com padrão (`find`, `match`, `gmatch`, `gsub`).
pub const SUJEITO_MAXIMO: usize = 256 * 1024;
/// Tamanho máximo do padrão (ou do texto procurado, na busca literal).
pub const PADRAO_MAXIMO: usize = 1024;
/// Tamanho máximo do resultado de uma chamada a `string.rep`.
pub const REPETICAO_MAXIMA: usize = 1024 * 1024;

/// Troca as funções de padrão e `string.rep` da tabela `string` pelas versões controladas.
/// Deve rodar antes de `lua.sandbox(true)`, que deixa a tabela somente leitura.
pub(crate) fn instalar(lua: &Lua) -> Result<()> {
    let string: Table = lua.globals().get("string")?;

    // (nome, posição do padrão: sempre 1; posição do argumento "plain": só no `find`)
    for (nome, plain) in [
        ("find", Some(3usize)),
        ("match", None),
        ("gmatch", None),
        ("gsub", None),
    ] {
        let original: Function = string.get(nome)?;
        let rotulo = format!("string.{nome}");
        let nova = lua.create_function(move |_, args: Variadic<Value>| -> Result<MultiValue> {
            conferir_busca(&rotulo, &args, plain)?;
            original.call(args)
        })?;
        string.set(nome, nova)?;
    }

    let rep: Function = string.get("rep")?;
    let nova = lua.create_function(move |_, args: Variadic<Value>| -> Result<MultiValue> {
        conferir_repeticao(&args)?;
        rep.call(args)
    })?;
    string.set("rep", nova)?;
    Ok(())
}

/// Tamanho em bytes de um valor de texto; números e outros tipos não contam (a função
/// original os converte ou recusa).
fn tamanho(v: Option<&Value>) -> usize {
    match v {
        Some(Value::String(s)) => s.as_bytes().len(),
        _ => 0,
    }
}

fn verdadeiro(v: Option<&Value>) -> bool {
    !matches!(v, None | Some(Value::Nil | Value::Boolean(false)))
}

fn conferir_busca(rotulo: &str, args: &[Value], plain: Option<usize>) -> Result<()> {
    let literal = plain.is_some_and(|i| verdadeiro(args.get(i)));

    if tamanho(args.first()) > SUJEITO_MAXIMO {
        return Err(Error::runtime(format!(
            "{rotulo}: o texto onde se busca é grande demais ({} KiB; o máximo é {} KiB). \
             Busque só no pedaço que interessa (por exemplo, o texto de um seletor)",
            tamanho(args.first()).div_ceil(1024),
            SUJEITO_MAXIMO / 1024
        )));
    }
    if tamanho(args.get(1)) > PADRAO_MAXIMO {
        return Err(Error::runtime(format!(
            "{rotulo}: o padrão é grande demais (o máximo é {PADRAO_MAXIMO} bytes)"
        )));
    }
    if !literal && let Some(Value::String(p)) = args.get(1) {
        conferir_padrao(rotulo, p.as_bytes().as_ref())?;
    }
    Ok(())
}

/// Recusa `%b` (pareamento, quadrático) e `%1`..`%9` (retroreferência, pode ser pior que
/// quadrático). `%%` é um `%` literal e não conta.
fn conferir_padrao(rotulo: &str, padrao: &[u8]) -> Result<()> {
    let mut i = 0;
    while i < padrao.len() {
        if padrao[i] == b'%' {
            match padrao.get(i + 1) {
                Some(b'b') => {
                    return Err(Error::runtime(format!(
                        "{rotulo}: o padrão `%b` não é permitido nos roteiros (pode travar \
                         a busca). Use `find(texto, literal, 1, true)` ou separe o texto \
                         por partes"
                    )));
                }
                Some(b'1'..=b'9') => {
                    return Err(Error::runtime(format!(
                        "{rotulo}: retroreferência (`%1`..`%9`) em padrão não é permitida \
                         nos roteiros (pode travar a busca). Compare as capturas depois, no \
                         próprio roteiro"
                    )));
                }
                _ => {}
            }
            i += 2;
        } else {
            i += 1;
        }
    }
    Ok(())
}

fn conferir_repeticao(args: &[Value]) -> Result<()> {
    let (Some(Value::String(s)), Some(n)) = (args.first(), args.get(1)) else {
        return Ok(()); // a função original reclama do tipo
    };
    let vezes = match n {
        Value::Integer(i) => (*i).max(0) as f64,
        Value::Number(f) if f.is_nan() => 0.0,
        Value::Number(f) => f.max(0.0),
        _ => return Ok(()),
    };
    let total = (s.as_bytes().len() as f64) * vezes.trunc();
    if total > REPETICAO_MAXIMA as f64 {
        return Err(Error::runtime(format!(
            "string.rep: o resultado passaria de {} KiB (o máximo por chamada)",
            REPETICAO_MAXIMA / 1024
        )));
    }
    Ok(())
}
