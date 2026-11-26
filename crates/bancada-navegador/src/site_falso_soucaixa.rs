//! O portal do SouCaixa de mentira: imita a ESTRUTURA do portal real do contracheque (observada
//! à mão, com dados do usuário nunca copiados) para o roteiro escrito `contracheque-soucaixa`
//! ser provado sem tocar o portal verdadeiro. **Nada de site real, nada de dado real**: CPF,
//! senha, nome, matrícula e valores são fictícios.
//!
//! Uma ÚNICA origem local imita os DOIS hosts do portal por caminhos: o site (`/`, `/home`,
//! `/contraCheque/MM/AAAA`) e o login (`/auth/realms/internet/...`, o Keycloak). O teste mapeia
//! os dois hosts lógicos do manifesto para esta origem.
//!
//! O fluxo, na ordem do portal real:
//!
//! 1. `GET /` sem sessão leva (302) ao login por CPF; com sessão, à `/home`.
//! 2. **CPF** (`#username`, botão `#button-submit`). CPF errado volta à mesma página.
//! 3. **Validação**: dois radios, `input[name="mail"]` (já marcado) e `input[name="tel"]`, cujo
//!    `id` carrega o contato MASCARADO; `button[name="login"]` ("Receber código") e o botão
//!    proibido "Quero alterar meus dados". Gera o código de 6 dígitos e o "envia" para a caixa
//!    de e-mail do portal ([`PortalDeSouCaixa::email_do_codigo`]).
//! 4. **Código** (`#codigo`, `button[name="login"]` "Enviar"; "Não recebi o código" é proibido).
//! 5. **Senha** (`#password`; o botão Entrar fica em
//!    `#template-section > form:nth-of-type(1) > div:nth-of-type(1) > button:nth-of-type(1)`;
//!    "Voltar" `#kc-cancel` e "Recuperar Senha" são proibidos). Senha errada volta à página da
//!    senha e conta em [`PortalDeSouCaixa::logins_recusados`].
//! 6. `/home` (`app-home`, com um aviso transitório "Ocorreu um erro inesperado" que some
//!    sozinho) e `/contraCheque/MM/AAAA`: alternador do calendário, calendário de meses com
//!    `aria-label` em português ("outubro de 2026"), meses fora do período DESABILITADOS, e a
//!    área `#interna` com "Voltar" (proibido) e "Baixar PDF" no caminho posicional do portal real.
//!    A URL NÃO muda ao escolher o mês, e a página abre SEMPRE no mês corrente do portal.
//! 7. "Baixar PDF" gera um download de verdade (`ContraCheque_MM_AAAA.pdf`, com texto extraível,
//!    sem senha). Com [`Config::pdf_bytes_novos_a_cada_baixa`], cada geração carimba o PDF com um
//!    número novo: os bytes mudam, o texto não.
//!
//! Tudo o que um roteiro que respeita a lista de bloqueio nunca alcança (Pedidos, Adiantamentos,
//! Auxílios, Perfil, Sair, alterar dados, reenviar código, Voltar, Recuperar Senha) entra em
//! [`PortalDeSouCaixa::acessos_proibidos`].
//!
//! O módulo está sempre compilado: o `testar` do roteiro escrito o usa em produção, contra
//! `127.0.0.1`, sem rede externa.

use crate::site_falso::{ler_pedido, responder, travar};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::SystemTime;

/// O host lógico do site (SPA do contracheque).
pub const HOST_DO_SITE: &str = "soucaixa.caixa.gov.br";
/// O host lógico do login (Keycloak, realm `internet`).
pub const HOST_DO_LOGIN: &str = "login.caixa.gov.br";
/// De quem vem o e-mail com o código.
pub const REMETENTE_DO_CODIGO: &str = "logincaixa@caixa.gov.br";
/// O assunto do e-mail com o código.
pub const ASSUNTO_DO_CODIGO: &str = "Login Caixa - Código de Validação";
/// O primeiro mês que o calendário do portal habilita (antes dele, desabilitado).
pub const PRIMEIRO_MES_HABILITADO: &str = "2026-05";
/// O CPF fictício que o portal aceita (só dígitos).
pub const CPF_FICTICIO: &str = "12345678909";
/// A senha fictícia que o portal aceita.
pub const SENHA_FICTICIA: &str = "SenhaFicticia#4821";

/// O prefixo do realm do login.
const REALM: &str = "/auth/realms/internet";

/// Os nomes dos meses, em minúsculas (como no `aria-label` do calendário).
const NOMES: [&str; 12] = [
    "janeiro",
    "fevereiro",
    "março",
    "abril",
    "maio",
    "junho",
    "julho",
    "agosto",
    "setembro",
    "outubro",
    "novembro",
    "dezembro",
];

/// Como o portal se comporta.
#[derive(Debug, Clone)]
pub struct Config {
    /// O CPF aceito (só dígitos; o formulário tira a pontuação).
    pub cpf: String,
    /// A senha aceita.
    pub senha: String,
    /// O mês corrente do portal (`AAAA-MM`): onde a página abre e o último mês habilitado.
    pub mes_corrente: String,
    /// Os meses com contracheque publicado (`AAAA-MM`).
    pub meses_publicados: Vec<String>,
    /// Cada geração do PDF leva um carimbo novo: os bytes mudam, o texto extraído não.
    pub pdf_bytes_novos_a_cada_baixa: bool,
    /// O portal recusa toda senha, até a certa.
    pub rejeitar_senha: bool,
}

impl Config {
    /// Credenciais fictícias dadas pelo teste, com o `mes_corrente` do portal e o mês corrente
    /// como único publicado.
    #[must_use]
    pub fn nova(cpf: &str, senha: &str, mes_corrente: &str) -> Self {
        Self {
            cpf: cpf.chars().filter(char::is_ascii_digit).collect(),
            senha: senha.to_owned(),
            mes_corrente: mes_corrente.to_owned(),
            meses_publicados: vec![mes_corrente.to_owned()],
            pdf_bytes_novos_a_cada_baixa: false,
            rejeitar_senha: false,
        }
    }

    /// Troca os meses publicados (`AAAA-MM`; o que não tiver esse formato é ignorado).
    #[must_use]
    pub fn com_meses_publicados(mut self, meses: &[&str]) -> Self {
        self.meses_publicados = meses.iter().map(|m| (*m).to_owned()).collect();
        self
    }

    /// Liga ou desliga o carimbo novo a cada geração do PDF.
    #[must_use]
    pub fn com_pdf_bytes_novos_a_cada_baixa(mut self, ligado: bool) -> Self {
        self.pdf_bytes_novos_a_cada_baixa = ligado;
        self
    }

    /// Liga ou desliga a recusa de toda senha.
    #[must_use]
    pub fn com_rejeitar_senha(mut self, ligado: bool) -> Self {
        self.rejeitar_senha = ligado;
        self
    }
}

#[derive(Debug)]
struct Estado {
    config: Config,
    /// O último código gerado e quando (a caixa de e-mail dos testes o lê daqui).
    codigo: Mutex<Option<(String, SystemTime)>>,
    baixados: Mutex<Vec<String>>,
    proibidos: Mutex<Vec<String>>,
    logins: AtomicUsize,
    logins_recusados: AtomicUsize,
    codigos_recusados: AtomicUsize,
    /// Quantos PDFs já foram gerados (o carimbo de cada um).
    geracoes: AtomicUsize,
    parar: AtomicBool,
}

/// O portal do SouCaixa de mentira. Encerra o servidor no `Drop`.
#[derive(Debug)]
pub struct PortalDeSouCaixa {
    porta: u16,
    estado: Arc<Estado>,
    fio: Option<JoinHandle<()>>,
}

/// `AAAA-MM` com mês de 01 a 12.
fn mes_valido(m: &str) -> bool {
    let b = m.as_bytes();
    b.len() == 7
        && b[..4].iter().all(u8::is_ascii_digit)
        && b[4] == b'-'
        && matches!(
            &m[5..],
            "01" | "02" | "03" | "04" | "05" | "06" | "07" | "08" | "09" | "10" | "11" | "12"
        )
}

/// O nome do mês (`AAAA-MM`) com a inicial maiúscula e o ano: "Outubro 2026".
#[must_use]
pub fn titulo_do_mes(mes: &str) -> String {
    if !mes_valido(mes) {
        return mes.to_owned();
    }
    let numero: usize = mes[5..].parse().unwrap_or(1);
    let nome = NOMES[numero.clamp(1, 12) - 1];
    let mut letras = nome.chars();
    let inicial: String = letras
        .next()
        .map(|c| c.to_uppercase().collect())
        .unwrap_or_default();
    format!("{inicial}{} {}", letras.as_str(), &mes[..4])
}

/// Seis dígitos aleatórios.
fn codigo_aleatorio() -> String {
    let mut b = [0_u8; 4];
    // Sem fonte de aleatoriedade, cai num código fixo: é um site de mentira.
    if getrandom::fill(&mut b).is_err() {
        return "000000".to_owned();
    }
    format!("{:06}", u32::from_le_bytes(b) % 1_000_000)
}

impl PortalDeSouCaixa {
    /// Sobe o portal numa porta livre de `127.0.0.1`.
    ///
    /// # Errors
    /// Se não conseguir abrir a porta.
    pub fn iniciar(mut config: Config) -> std::io::Result<Self> {
        config.meses_publicados.retain(|m| mes_valido(m));
        config.cpf.retain(|c| c.is_ascii_digit());
        if !mes_valido(&config.mes_corrente) {
            config.mes_corrente = PRIMEIRO_MES_HABILITADO.to_owned();
        }
        let escuta = TcpListener::bind(SocketAddr::from(([127, 0, 0, 1], 0)))?;
        let porta = escuta.local_addr()?.port();
        let estado = Arc::new(Estado {
            config,
            codigo: Mutex::new(None),
            baixados: Mutex::new(Vec::new()),
            proibidos: Mutex::new(Vec::new()),
            logins: AtomicUsize::new(0),
            logins_recusados: AtomicUsize::new(0),
            codigos_recusados: AtomicUsize::new(0),
            geracoes: AtomicUsize::new(0),
            parar: AtomicBool::new(false),
        });
        let e = Arc::clone(&estado);
        let fio = std::thread::spawn(move || {
            for conexao in escuta.incoming() {
                if e.parar.load(Ordering::SeqCst) {
                    break;
                }
                let Ok(c) = conexao else { continue };
                let e2 = Arc::clone(&e);
                std::thread::spawn(move || {
                    let _ = atender(c, &e2);
                });
            }
        });
        Ok(Self {
            porta,
            estado,
            fio: Some(fio),
        })
    }

    /// A URL base, sem barra no fim (`http://127.0.0.1:porta`).
    #[must_use]
    pub fn url_base(&self) -> String {
        format!("http://127.0.0.1:{}", self.porta)
    }

    /// A porta.
    #[must_use]
    pub fn porta(&self) -> u16 {
        self.porta
    }

    /// Os dois hosts lógicos que esta origem imita: o do site e o do login.
    #[must_use]
    pub fn hosts_logicos(&self) -> [&'static str; 2] {
        [HOST_DO_SITE, HOST_DO_LOGIN]
    }

    /// O último código gerado ao pedir "Receber código".
    #[must_use]
    pub fn codigo_gerado(&self) -> Option<String> {
        travar(&self.estado.codigo).as_ref().map(|(c, _)| c.clone())
    }

    /// Quando o último código foi gerado.
    #[must_use]
    pub fn instante_do_codigo(&self) -> Option<SystemTime> {
        travar(&self.estado.codigo).as_ref().map(|(_, t)| *t)
    }

    /// O corpo do e-mail que o portal "mandou" com o último código: o que a caixa de e-mail dos
    /// testes devolve, recebido em [`Self::instante_do_codigo`].
    #[must_use]
    pub fn email_do_codigo(&self) -> Option<(String, SystemTime)> {
        travar(&self.estado.codigo).as_ref().map(|(c, t)| {
            (
                format!("Código de ativação: {c}\nO código é válido por 5 minutos."),
                *t,
            )
        })
    }

    /// O mês (`AAAA-MM`) foi baixado pelo menos uma vez?
    #[must_use]
    pub fn baixou(&self, mes: &str) -> bool {
        travar(&self.estado.baixados).iter().any(|m| m == mes)
    }

    /// Quantas vezes o mês (`AAAA-MM`) foi baixado.
    #[must_use]
    pub fn baixadas_de(&self, mes: &str) -> usize {
        travar(&self.estado.baixados)
            .iter()
            .filter(|m| *m == mes)
            .count()
    }

    /// Quantos downloads foram servidos (de qualquer mês).
    #[must_use]
    pub fn downloads(&self) -> usize {
        travar(&self.estado.baixados).len()
    }

    /// Logins completos (CPF, código e senha certos).
    #[must_use]
    pub fn logins(&self) -> usize {
        self.estado.logins.load(Ordering::SeqCst)
    }

    /// Logins recusados (CPF ou senha errados, ou a recusa total de senha).
    #[must_use]
    pub fn logins_recusados(&self) -> usize {
        self.estado.logins_recusados.load(Ordering::SeqCst)
    }

    /// Códigos recusados.
    #[must_use]
    pub fn codigos_recusados(&self) -> usize {
        self.estado.codigos_recusados.load(Ordering::SeqCst)
    }

    /// Tudo o que um roteiro que respeita a lista de bloqueio nunca alcança (`MÉTODO caminho`).
    #[must_use]
    pub fn acessos_proibidos(&self) -> Vec<String> {
        travar(&self.estado.proibidos).clone()
    }
}

impl Drop for PortalDeSouCaixa {
    fn drop(&mut self) {
        self.estado.parar.store(true, Ordering::SeqCst);
        // acorda o accept
        let _ = TcpStream::connect(SocketAddr::from(([127, 0, 0, 1], self.porta)));
        if let Some(f) = self.fio.take() {
            let _ = f.join();
        }
    }
}

// ---------------------------------------------------------------------------------------------
// O PDF fictício
// ---------------------------------------------------------------------------------------------

/// Um valor em centavos como "R$ 1.234,56".
fn dinheiro(centavos: i64) -> String {
    let inteiro = centavos / 100;
    let resto = centavos % 100;
    let mut milhar = String::new();
    let digitos = inteiro.to_string();
    for (i, c) in digitos.chars().enumerate() {
        if i > 0 && (digitos.len() - i).is_multiple_of(3) {
            milhar.push('.');
        }
        milhar.push(c);
    }
    format!("R$ {milhar},{resto:02}")
}

/// Os valores fictícios do mês, em centavos: `(base, gratificação, créditos, INSS, IRRF, plano,
/// descontos, líquido)`. Variam com o mês, para o texto de um mês nunca ser o de outro.
fn valores_do_mes(mes: &str) -> [i64; 8] {
    let numero: i64 = mes.get(5..7).and_then(|m| m.parse().ok()).unwrap_or(1);
    let base = 500_000 + 1_000 * numero;
    let gratificacao = 120_000 + 500 * numero;
    let creditos = base + gratificacao;
    let inss = base * 11 / 100;
    let irrf = (creditos - inss) * 15 / 100;
    let plano = 45_000;
    let descontos = inss + irrf + plano;
    [
        base,
        gratificacao,
        creditos,
        inss,
        irrf,
        plano,
        descontos,
        creditos - descontos,
    ]
}

/// Escapa um texto para uma string literal do PDF (`WinAnsiEncoding`): parênteses e barra com
/// barra, e o que passa de ASCII (Latin-1) como escape octal; o arquivo inteiro fica ASCII.
fn escapar_para_pdf(texto: &str) -> String {
    let mut saida = String::new();
    for c in texto.chars() {
        match c {
            '(' | ')' | '\\' => {
                saida.push('\\');
                saida.push(c);
            }
            c if c.is_ascii() && !c.is_ascii_control() => saida.push(c),
            c if (c as u32) < 256 => saida.push_str(&format!("\\{:03o}", c as u32)),
            _ => saida.push('?'),
        }
    }
    saida
}

/// O conteúdo textual da página, uma linha por item: o mesmo texto que o `pdftotext` devolve.
fn linhas_do_contracheque(mes: &str) -> Vec<String> {
    let [
        base,
        gratificacao,
        creditos,
        inss,
        irrf,
        plano,
        descontos,
        liquido,
    ] = valores_do_mes(mes);
    let titulo = titulo_do_mes(mes);
    vec![
        format!("CONTRACHEQUE FICTÍCIO - {titulo}"),
        "Documento de teste do SBF. Nenhum dado aqui é real.".to_owned(),
        "Empregado: Fulano de Tal    Matrícula: CX-0001".to_owned(),
        "Lotação: Agência Exemplo 0001    Cargo: Analista Fictício".to_owned(),
        format!("Competência: {titulo}    Folha: mensal"),
        "CRÉDITOS".to_owned(),
        format!("0001  Salário base ............ {}", dinheiro(base)),
        format!("0002  Gratificação de função .. {}", dinheiro(gratificacao)),
        format!("Total de créditos ............. {}", dinheiro(creditos)),
        "DESCONTOS".to_owned(),
        format!("0101  Contribuição previdenciária .. {}", dinheiro(inss)),
        format!("0102  Imposto de renda retido ...... {}", dinheiro(irrf)),
        format!("0103  Plano de saúde ............... {}", dinheiro(plano)),
        format!("Total de descontos ............ {}", dinheiro(descontos)),
        format!("Salário bruto: {}", dinheiro(creditos)),
        format!("Salário líquido: {}", dinheiro(liquido)),
        format!("Este contracheque de {titulo} foi gerado para a demonstração do roteiro."),
    ]
}

/// O fluxo de conteúdo (as linhas de texto) da página do PDF.
fn fluxo_do_contracheque(mes: &str) -> String {
    let mut f = String::from("BT /F1 11 Tf 56 780 Td 16 TL\n");
    for (i, l) in linhas_do_contracheque(mes).iter().enumerate() {
        if i > 0 {
            f.push_str("T*\n");
        }
        f.push_str(&format!("({}) Tj\n", escapar_para_pdf(l)));
    }
    f.push_str("ET");
    f
}

/// Um PDF válido e pequeno do contracheque FICTÍCIO de `mes` (`AAAA-MM`): uma página, Helvetica,
/// texto extraível, sem senha. Com `carimbo`, o dicionário de informações do PDF leva esse texto:
/// os bytes mudam e o texto extraído não (o contracheque gerado em outra hora).
#[must_use]
pub fn pdf_do_contracheque(mes: &str, carimbo: Option<&str>) -> Vec<u8> {
    let fluxo = fluxo_do_contracheque(mes);
    let info = format!(
        "<< /Title (Contracheque ficticio) /Producer (SouCaixa Ficticio){} >>",
        carimbo.map_or_else(String::new, |c| format!(
            " /CreationDate (D:{})",
            escapar_para_pdf(c)
        ))
    );
    let objetos = [
        "<< /Type /Catalog /Pages 2 0 R >>".to_owned(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_owned(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 595 842] /Contents 4 0 R \
         /Resources << /Font << /F1 5 0 R >> >> >>"
            .to_owned(),
        format!("<< /Length {} >>\nstream\n{fluxo}\nendstream", fluxo.len()),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>"
            .to_owned(),
        info,
    ];
    let mut pdf = String::from("%PDF-1.4\n");
    let mut posicoes = Vec::new();
    for (i, o) in objetos.iter().enumerate() {
        posicoes.push(pdf.len());
        pdf.push_str(&format!("{} 0 obj\n{o}\nendobj\n", i + 1));
    }
    let xref = pdf.len();
    pdf.push_str(&format!(
        "xref\n0 {}\n0000000000 65535 f \n",
        objetos.len() + 1
    ));
    for p in posicoes {
        pdf.push_str(&format!("{p:010} 00000 n \n"));
    }
    pdf.push_str(&format!(
        "trailer\n<< /Size {} /Root 1 0 R /Info 6 0 R >>\nstartxref\n{xref}\n%%EOF\n",
        objetos.len() + 1
    ));
    pdf.into_bytes()
}

// ---------------------------------------------------------------------------------------------
// As páginas
// ---------------------------------------------------------------------------------------------

const ESTILO: &str = "body{font-family:sans-serif;max-width:760px;margin:1.5em auto;padding:0 12px}\
    input,button{font-size:16px;padding:6px 10px;margin:4px 0}\
    [hidden]{display:none!important}\
    app-home,mat-datepicker,mat-calendar-header,mat-year-view{display:block}\
    mat-datepicker-toggle{display:inline-block}\
    mat-datepicker{border:1px solid #888;padding:8px;margin:6px 0;background:#fff}\
    mat-calendar-header button{margin-right:6px}\
    mat-year-view table{border-collapse:collapse}\
    mat-year-view td{padding:2px}mat-year-view button{min-width:64px}\
    .aviso{background:#fdd;padding:6px;margin:6px 0}\
    .cartao{border:1px solid #999;padding:10px;margin:10px 0}\
    nav a{margin-right:12px}";

fn pagina(titulo: &str, corpo: &str) -> Vec<u8> {
    format!(
        "<!doctype html><html lang=\"pt-BR\"><head><meta charset=\"utf-8\"><title>{titulo}</title>\
         <style>{ESTILO}</style></head><body>{corpo}</body></html>"
    )
    .into_bytes()
}

/// O e-mail mascarado que o `id` do radio carrega (o portal real nunca mostra o inteiro).
const EMAIL_MASCARADO: &str = "f*****@exemplo.test";
/// O telefone mascarado.
const TELEFONE_MASCARADO: &str = "(**) *****-1234";

fn pagina_do_cpf(erro: bool) -> Vec<u8> {
    let aviso = if erro {
        "<p class=\"alert\">CPF inválido ou não encontrado.</p>"
    } else {
        ""
    };
    pagina(
        "Login CAIXA",
        &format!(
            "<div id=\"kc-content\"><h1 id=\"kc-page-title\">Login CAIXA - Informe seu CPF</h1>{aviso}\
             <form id=\"kc-form-login\" method=\"post\" action=\"{REALM}/login\">\
             <label for=\"username\">CPF</label><br>\
             <input id=\"username\" name=\"username\" type=\"text\" autocomplete=\"username\"><br>\
             <button id=\"button-submit\" type=\"submit\">Próximo</button></form></div>"
        ),
    )
}

fn pagina_da_validacao() -> Vec<u8> {
    pagina(
        "Validação",
        &format!(
            "<div id=\"kc-content\"><h1>Validação</h1>\
             <p>Escolha o email ou telefone para receber o código de validação</p>\
             <form id=\"kc-form-validacao\" method=\"post\" action=\"{REALM}/validacao\">\
             <div><label><input type=\"radio\" name=\"mail\" id=\"{EMAIL_MASCARADO}\" checked> \
             {EMAIL_MASCARADO}</label></div>\
             <div><label><input type=\"radio\" name=\"tel\" id=\"{TELEFONE_MASCARADO}\"> \
             {TELEFONE_MASCARADO}</label></div>\
             <button type=\"submit\" name=\"login\">Receber código</button> \
             <button type=\"submit\" name=\"alterar\" formaction=\"{REALM}/alterar-dados\">\
             Quero alterar meus dados</button></form></div>"
        ),
    )
}

fn pagina_do_codigo(erro: bool) -> Vec<u8> {
    let aviso = if erro {
        "<p class=\"alert\">Código de validação inválido.</p>"
    } else {
        ""
    };
    pagina(
        "Código de validação",
        &format!(
            "<div id=\"kc-content\"><h1>Validação</h1>{aviso}\
             <p>Por favor, insira o código de validação recebido</p>\
             <form id=\"kc-form-codigo\" method=\"post\" action=\"{REALM}/codigo\">\
             <input id=\"codigo\" name=\"codigo\" type=\"text\" autocomplete=\"one-time-code\"><br>\
             <button type=\"submit\" name=\"login\">Enviar</button> \
             <button type=\"submit\" name=\"reenviar\" formaction=\"{REALM}/reenviar\">\
             Não recebi o código de validação</button></form></div>"
        ),
    )
}

fn pagina_da_senha(erro: bool) -> Vec<u8> {
    let aviso = if erro {
        "<p class=\"alert\">Senha inválida.</p>"
    } else {
        ""
    };
    // O botão Entrar é o 1º botão da 1ª `div` do 1º `form` de `#template-section`.
    pagina(
        "Senha",
        &format!(
            "<div id=\"template-section\"><h1>Login CAIXA</h1><p>Informe sua senha:</p>{aviso}\
             <form id=\"kc-form-senha\" method=\"post\" action=\"{REALM}/senha\">\
             <div><label for=\"password\">Senha</label><br>\
             <input id=\"password\" name=\"password\" type=\"password\" autocomplete=\"current-password\"><br>\
             <button type=\"submit\">Entrar</button></div></form>\
             <form method=\"post\" action=\"{REALM}/cancelar\">\
             <button id=\"kc-cancel\" type=\"submit\" name=\"cancel\">Voltar</button></form>\
             <p><a href=\"{REALM}/recuperar\">Recuperar Senha</a></p></div>"
        ),
    )
}

fn rota_do_contracheque(mes: &str) -> String {
    format!("/contraCheque/{}/{}", &mes[5..], &mes[..4])
}

fn pagina_da_home(mes_corrente: &str) -> Vec<u8> {
    let rota = rota_do_contracheque(mes_corrente);
    pagina(
        "SouCaixa",
        &format!(
            "<app-home>\
             <div class=\"aviso\" id=\"aviso-transitorio\">Ocorreu um erro inesperado</div>\
             <h1>SouCaixa</h1>\
             <nav><a href=\"/pedidos\">Pedidos</a><a href=\"/adiantamentos\">Adiantamentos</a>\
             <a href=\"/auxilios\">Auxílios</a><a href=\"/perfil\">Perfil</a>\
             <a href=\"/sair\">Sair</a></nav>\
             <div><a id=\"ir-contracheque\" href=\"{rota}\">Ir para Contracheque</a></div>\
             <div class=\"icone\"><a href=\"{rota}\"><span>Contracheque</span></a></div>\
             </app-home>\
             <script>setTimeout(function(){{var a=document.getElementById('aviso-transitorio');\
             if(a)a.remove();}},1500);</script>"
        ),
    )
}

/// O conteúdo de um mês publicado, dentro de `#interna`: 1º `div` = o cartão; nele, o 3º `div`
/// tem 8 `div`s, e o 8º é o rodapé, com "Voltar" (1º `div`) e "Baixar PDF" (2º `div`).
fn cartao_do_mes(mes: &str) -> String {
    let [
        base,
        gratificacao,
        creditos,
        inss,
        irrf,
        plano,
        descontos,
        liquido,
    ] = valores_do_mes(mes);
    let titulo = titulo_do_mes(mes);
    format!(
        "<div class=\"cartao\">\
         <div class=\"titulo\"><h2>{titulo}</h2></div>\
         <div class=\"empregado\">Empregado: Fulano de Tal | Matrícula: CX-0001</div>\
         <div class=\"corpo\">\
         <div><strong>Créditos</strong></div>\
         <div>Salário base {} | Gratificação de função {}</div>\
         <div>Total de créditos {}</div>\
         <div><strong>Descontos</strong></div>\
         <div>Contribuição previdenciária {} | Imposto de renda {} | Plano de saúde {}</div>\
         <div>Total de descontos {}</div>\
         <div>Salário bruto {} | Salário líquido {}</div>\
         <div class=\"rodape\">\
         <div><button type=\"button\" data-acao=\"voltar\">Voltar</button></div>\
         <div><button type=\"button\" data-acao=\"baixar\" aria-label=\"Clique aqui para baixar seu contracheque\" \
         title=\"Clique aqui para baixar seu contracheque\">Baixar PDF</button></div>\
         </div></div></div>",
        dinheiro(base),
        dinheiro(gratificacao),
        dinheiro(creditos),
        dinheiro(inss),
        dinheiro(irrf),
        dinheiro(plano),
        dinheiro(descontos),
        dinheiro(creditos),
        dinheiro(liquido),
    )
}

/// O comportamento da SPA: o calendário de meses, a escolha do mês (sem mudar a URL) e o
/// download. Os marcadores `__CORRENTE__` e `__MINIMO__` são trocados pelo servidor.
const SCRIPT_DO_CONTRACHEQUE: &str = "(function(){\
var CORRENTE='__CORRENTE__',MINIMO='__MINIMO__';\
var NOMES=['janeiro','fevereiro','março','abril','maio','junho','julho','agosto','setembro','outubro','novembro','dezembro'];\
var ano=parseInt(CORRENTE.slice(0,4),10),selecionado=null;\
var dp=document.querySelector('mat-datepicker'),cab=document.querySelector('mat-calendar-header'),\
vista=document.querySelector('mat-year-view'),area=document.getElementById('area');\
function chave(a,m){return a+'-'+(m<10?'0':'')+m;}\
function desenhar(){cab.querySelector('button').textContent=String(ano);\
var h='<table><tbody>';\
for(var l=0;l<3;l++){h+='<tr>';for(var c=0;c<4;c++){var m=l*4+c+1,k=chave(ano,m);\
var fora=(k<MINIMO||k>CORRENTE);\
h+='<td><button type=\"button\" aria-label=\"'+NOMES[m-1]+' de '+ano+'\"'+\
(fora?' disabled aria-disabled=\"true\"':' aria-disabled=\"false\"')+' data-mes=\"'+k+'\">'+\
NOMES[m-1].slice(0,3).toUpperCase()+'</button></td>';}h+='</tr>';}\
vista.innerHTML=h+'</tbody></table>';}\
function escolher(k){dp.hidden=true;var velho=document.getElementById('interna');\
if(velho)velho.remove();selecionado=k;\
setTimeout(function(){if(selecionado!==k)return;var t=document.getElementById('m-'+k);\
if(!t)return;var d=document.createElement('div');d.id='interna';d.innerHTML=t.innerHTML;\
area.appendChild(d);},300);}\
document.addEventListener('click',function(ev){var b=ev.target.closest&&ev.target.closest('button');\
if(!b)return;\
if(b.closest('mat-datepicker-toggle')){dp.hidden=!dp.hidden;if(!dp.hidden)desenhar();return;}\
var r=b.getAttribute('aria-label');\
if(r==='Previous year'){ano--;desenhar();return;}\
if(r==='Next year'){ano++;desenhar();return;}\
if(b.closest('mat-year-view')){if(!b.disabled)escolher(b.getAttribute('data-mes'));return;}\
var a=b.getAttribute('data-acao');\
if(a==='baixar'&&selecionado){location.href='/api/contraCheque/'+selecionado+'.pdf';return;}\
if(a==='voltar'){fetch('/api/voltar',{method:'POST'});}});\
desenhar();escolher(CORRENTE);})();";

fn pagina_do_contracheque(config: &Config) -> Vec<u8> {
    let mut modelos = String::new();
    for mes in &config.meses_publicados {
        modelos.push_str(&format!(
            "<template id=\"m-{mes}\">{}</template>",
            cartao_do_mes(mes)
        ));
    }
    let script = SCRIPT_DO_CONTRACHEQUE
        .replace("__CORRENTE__", &config.mes_corrente)
        .replace("__MINIMO__", PRIMEIRO_MES_HABILITADO);
    pagina(
        "Contracheque",
        &format!(
            "<h1>Contracheque</h1>\
             <div id=\"matFormFieldDataTemplate\"><label>Consultar por mês e ano</label> \
             <mat-datepicker-toggle><button type=\"button\" aria-label=\"Open calendar\">&#128197;</button>\
             </mat-datepicker-toggle></div>\
             <mat-datepicker id=\"mat-datepicker-0\" hidden>\
             <mat-calendar-header><button type=\"button\" aria-label=\"Choose date\">0000</button>\
             <button type=\"button\" aria-label=\"Previous year\">&lt;</button>\
             <button type=\"button\" aria-label=\"Next year\">&gt;</button></mat-calendar-header>\
             <mat-year-view></mat-year-view></mat-datepicker>\
             <div id=\"area\"></div>{modelos}<script>{script}</script>"
        ),
    )
}

// ---------------------------------------------------------------------------------------------
// O servidor
// ---------------------------------------------------------------------------------------------

fn tem(cookies: &str, nome: &str) -> bool {
    cookies.split(';').any(|c| c.trim() == format!("{nome}=ok"))
}

fn decodificar(v: &str) -> String {
    let b = v.as_bytes();
    let mut saida = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'+' => saida.push(b' '),
            b'%' if i + 2 < b.len() => {
                let h = std::str::from_utf8(&b[i + 1..i + 3])
                    .ok()
                    .and_then(|h| u8::from_str_radix(h, 16).ok());
                if let Some(x) = h {
                    saida.push(x);
                    i += 2;
                } else {
                    saida.push(b'%');
                }
            }
            x => saida.push(x),
        }
        i += 1;
    }
    String::from_utf8_lossy(&saida).into_owned()
}

fn campo(corpo: &str, nome: &str) -> Option<String> {
    corpo
        .split('&')
        .filter_map(|p| p.split_once('=').or(Some((p, ""))))
        .find(|(k, _)| *k == nome)
        .map(|(_, v)| decodificar(v))
}

fn redirecionar(c: &mut TcpStream, para: &str, cookie: Option<&str>) -> std::io::Result<()> {
    let extra = format!(
        "Location: {para}\r\nCache-Control: no-store\r\n{}",
        cookie
            .map(|k| format!("Set-Cookie: {k}; Path=/\r\n"))
            .unwrap_or_default()
    );
    responder(c, "302 Found", &extra, "text/plain", b"")
}

fn html(c: &mut TcpStream, corpo: &[u8]) -> std::io::Result<()> {
    responder(
        c,
        "200 OK",
        "Cache-Control: no-store\r\n",
        "text/html; charset=utf-8",
        corpo,
    )
}

fn atender(mut c: TcpStream, e: &Estado) -> std::io::Result<()> {
    let Some(p) = ler_pedido(&mut c)? else {
        return Ok(());
    };
    let rota = p.caminho.split('?').next().unwrap_or("/").to_owned();
    let consulta = p.caminho.split_once('?').map_or("", |(_, q)| q).to_owned();
    let metodo = p.metodo.as_str();
    let com_cpf = tem(&p.cookies, "cpf");
    let com_codigo = tem(&p.cookies, "codigo");
    let logado = tem(&p.cookies, "sessao");
    let login = |r: &str| format!("{REALM}/{r}");
    match (metodo, rota.as_str()) {
        ("GET", "/") => {
            let para = if logado {
                "/home".to_owned()
            } else {
                login("login")
            };
            redirecionar(&mut c, &para, None)
        }
        // ---- o login (Keycloak) ------------------------------------------------------------
        ("GET", r) if r == login("login") => {
            if logado {
                redirecionar(&mut c, "/home", None)
            } else {
                html(&mut c, &pagina_do_cpf(consulta.contains("erro=1")))
            }
        }
        ("POST", r) if r == login("login") => {
            let cpf: String = campo(&p.corpo, "username")
                .unwrap_or_default()
                .chars()
                .filter(char::is_ascii_digit)
                .collect();
            if !cpf.is_empty() && cpf == e.config.cpf {
                redirecionar(&mut c, &login("validacao"), Some("cpf=ok"))
            } else {
                e.logins_recusados.fetch_add(1, Ordering::SeqCst);
                redirecionar(&mut c, &format!("{}?erro=1", login("login")), None)
            }
        }
        ("GET", r) if r == login("validacao") => {
            if com_cpf {
                html(&mut c, &pagina_da_validacao())
            } else {
                redirecionar(&mut c, &login("login"), None)
            }
        }
        ("POST", r) if r == login("validacao") => {
            if !com_cpf {
                return redirecionar(&mut c, &login("login"), None);
            }
            // O radio de SMS não manda nada para a caixa de e-mail: só o `mail` gera o código.
            if campo(&p.corpo, "mail").is_some() {
                *travar(&e.codigo) = Some((codigo_aleatorio(), SystemTime::now()));
                redirecionar(&mut c, &login("codigo"), None)
            } else {
                redirecionar(&mut c, &login("validacao"), None)
            }
        }
        ("GET", r) if r == login("codigo") => {
            if com_cpf {
                html(&mut c, &pagina_do_codigo(consulta.contains("erro=1")))
            } else {
                redirecionar(&mut c, &login("login"), None)
            }
        }
        ("POST", r) if r == login("codigo") => {
            if !com_cpf {
                return redirecionar(&mut c, &login("login"), None);
            }
            let enviado = campo(&p.corpo, "codigo").unwrap_or_default();
            let esperado = travar(&e.codigo).as_ref().map(|(c, _)| c.clone());
            if !enviado.is_empty() && esperado.as_deref() == Some(enviado.as_str()) {
                redirecionar(&mut c, &login("senha"), Some("codigo=ok"))
            } else {
                e.codigos_recusados.fetch_add(1, Ordering::SeqCst);
                redirecionar(&mut c, &format!("{}?erro=1", login("codigo")), None)
            }
        }
        ("GET", r) if r == login("senha") => {
            if com_codigo {
                html(&mut c, &pagina_da_senha(consulta.contains("erro=1")))
            } else {
                redirecionar(&mut c, &login("login"), None)
            }
        }
        ("POST", r) if r == login("senha") => {
            if !com_codigo {
                return redirecionar(&mut c, &login("login"), None);
            }
            let senha = campo(&p.corpo, "password").unwrap_or_default();
            if !e.config.rejeitar_senha && senha == e.config.senha {
                e.logins.fetch_add(1, Ordering::SeqCst);
                redirecionar(&mut c, "/home", Some("sessao=ok"))
            } else {
                e.logins_recusados.fetch_add(1, Ordering::SeqCst);
                redirecionar(&mut c, &format!("{}?erro=1", login("senha")), None)
            }
        }
        // ---- o que nenhum roteiro alcança ---------------------------------------------------
        (m, r)
            if [
                login("alterar-dados"),
                login("reenviar"),
                login("cancelar"),
                login("recuperar"),
                "/pedidos".to_owned(),
                "/adiantamentos".to_owned(),
                "/auxilios".to_owned(),
                "/perfil".to_owned(),
                "/sair".to_owned(),
                "/api/voltar".to_owned(),
            ]
            .iter()
            .any(|proibida| r == proibida) =>
        {
            travar(&e.proibidos).push(format!("{m} {r}"));
            html(&mut c, &pagina("Feito", "<p>Operação realizada.</p>"))
        }
        // ---- o site ----------------------------------------------------------------------------
        ("GET", "/home") => {
            if logado {
                html(&mut c, &pagina_da_home(&e.config.mes_corrente))
            } else {
                redirecionar(&mut c, &login("login"), None)
            }
        }
        ("GET", r) if r.starts_with("/contraCheque/") => {
            if logado {
                html(&mut c, &pagina_do_contracheque(&e.config))
            } else {
                redirecionar(&mut c, &login("login"), None)
            }
        }
        ("GET", r) if r.starts_with("/api/contraCheque/") && r.ends_with(".pdf") => {
            if !logado {
                return redirecionar(&mut c, &login("login"), None);
            }
            let mes = &r["/api/contraCheque/".len()..r.len() - ".pdf".len()];
            if mes_valido(mes) && e.config.meses_publicados.iter().any(|m| m == mes) {
                travar(&e.baixados).push(mes.to_owned());
                let carimbo = e
                    .config
                    .pdf_bytes_novos_a_cada_baixa
                    .then(|| format!("geracao-{}", e.geracoes.fetch_add(1, Ordering::SeqCst) + 1));
                responder(
                    &mut c,
                    "200 OK",
                    &format!(
                        "Content-Disposition: attachment; filename=\"ContraCheque_{}_{}.pdf\"\r\n",
                        &mes[5..],
                        &mes[..4]
                    ),
                    "application/pdf",
                    &pdf_do_contracheque(mes, carimbo.as_deref()),
                )
            } else {
                responder(&mut c, "404 Not Found", "", "text/plain", b"nao encontrado")
            }
        }
        _ => responder(&mut c, "404 Not Found", "", "text/plain", b"nao encontrado"),
    }
}

#[cfg(test)]
mod testes {
    use super::*;
    use std::io::{Read, Write};
    use std::time::Duration;

    const MES: &str = "2026-11";

    struct Resposta {
        status: u16,
        cabecalho: String,
        corpo: Vec<u8>,
    }

    impl Resposta {
        fn texto(&self) -> String {
            String::from_utf8_lossy(&self.corpo).into_owned()
        }
        fn cabecalho_de(&self, nome: &str) -> Option<String> {
            self.cabecalho.lines().find_map(|l| {
                let (k, v) = l.split_once(':')?;
                k.trim()
                    .eq_ignore_ascii_case(nome)
                    .then(|| v.trim().to_owned())
            })
        }
        fn local(&self) -> Option<String> {
            self.cabecalho_de("location")
        }
        fn cookie(&self) -> Option<String> {
            self.cabecalho_de("set-cookie")
                .map(|c| c.split(';').next().unwrap_or("").to_owned())
        }
    }

    fn pedir(
        portal: &PortalDeSouCaixa,
        metodo: &str,
        caminho: &str,
        corpo: &str,
        cookies: &str,
    ) -> Resposta {
        let mut c = TcpStream::connect(("127.0.0.1", portal.porta())).unwrap();
        c.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
        write!(
            c,
            "{metodo} {caminho} HTTP/1.1\r\nHost: {HOST_DO_SITE}\r\nCookie: {cookies}\r\n\
             Content-Type: application/x-www-form-urlencoded\r\nContent-Length: {}\r\n\
             Connection: close\r\n\r\n{corpo}",
            corpo.len()
        )
        .unwrap();
        let mut bruto = Vec::new();
        c.read_to_end(&mut bruto).unwrap();
        let fim = bruto.windows(4).position(|w| w == b"\r\n\r\n").unwrap();
        let cabecalho = String::from_utf8_lossy(&bruto[..fim]).into_owned();
        let status = cabecalho.split(' ').nth(1).unwrap().parse().unwrap();
        Resposta {
            status,
            cabecalho,
            corpo: bruto[fim + 4..].to_vec(),
        }
    }

    fn portal(config: Config) -> PortalDeSouCaixa {
        PortalDeSouCaixa::iniciar(config).unwrap()
    }

    fn padrao() -> Config {
        Config::nova("123.456.789-09", SENHA_FICTICIA, MES).com_meses_publicados(&["2026-10", MES])
    }

    /// Faz o login inteiro por HTTP e devolve os cookies da sessão.
    fn entrar(p: &PortalDeSouCaixa) -> String {
        let r = pedir(
            p,
            "POST",
            &format!("{REALM}/login"),
            "username=123.456.789-09",
            "",
        );
        assert_eq!(r.cookie().as_deref(), Some("cpf=ok"));
        let cpf = "cpf=ok";
        let r = pedir(
            p,
            "POST",
            &format!("{REALM}/validacao"),
            "mail=on&login=",
            cpf,
        );
        assert_eq!(r.local().unwrap(), format!("{REALM}/codigo"));
        let codigo = p.codigo_gerado().unwrap();
        let r = pedir(
            p,
            "POST",
            &format!("{REALM}/codigo"),
            &format!("codigo={codigo}&login="),
            cpf,
        );
        assert_eq!(r.cookie().as_deref(), Some("codigo=ok"));
        let dois = "cpf=ok; codigo=ok";
        let r = pedir(
            p,
            "POST",
            &format!("{REALM}/senha"),
            "password=SenhaFicticia%234821",
            dois,
        );
        assert_eq!(r.local().as_deref(), Some("/home"));
        assert_eq!(r.cookie().as_deref(), Some("sessao=ok"));
        "cpf=ok; codigo=ok; sessao=ok".to_owned()
    }

    #[test]
    fn a_raiz_leva_ao_login_sem_sessao_e_a_home_com_sessao() {
        let p = portal(padrao());
        let r = pedir(&p, "GET", "/", "", "");
        assert_eq!(r.status, 302);
        assert_eq!(r.local().unwrap(), "/auth/realms/internet/login");
        let r = pedir(&p, "GET", "/", "", "sessao=ok");
        assert_eq!(r.local().unwrap(), "/home");
        assert_eq!(p.hosts_logicos(), [HOST_DO_SITE, HOST_DO_LOGIN]);
        assert!(p.url_base().starts_with("http://127.0.0.1:"));
    }

    #[test]
    fn a_pagina_do_cpf_tem_os_seletores_do_portal() {
        let p = portal(padrao());
        let r = pedir(&p, "GET", &format!("{REALM}/login"), "", "");
        assert_eq!(r.status, 200);
        let h = r.texto();
        assert!(h.contains("Login CAIXA - Informe seu CPF"));
        assert!(h.contains("<input id=\"username\" name=\"username\" type=\"text\""));
        assert!(h.contains("<button id=\"button-submit\" type=\"submit\">Próximo</button>"));
    }

    #[test]
    fn cpf_errado_volta_ao_cpf_e_conta_recusa() {
        let p = portal(padrao());
        for corpo in ["username=000.000.000-00", "username="] {
            let r = pedir(&p, "POST", &format!("{REALM}/login"), corpo, "");
            assert_eq!(r.local().unwrap(), format!("{REALM}/login?erro=1"));
            assert!(r.cookie().is_none());
        }
        assert_eq!(p.logins_recusados(), 2);
        let r = pedir(&p, "GET", &format!("{REALM}/login?erro=1"), "", "");
        assert!(r.texto().contains("CPF inválido"));
        assert!(r.texto().contains("id=\"username\""));
    }

    #[test]
    fn a_validacao_tem_os_dois_radios_com_id_mascarado_e_o_botao_proibido() {
        let p = portal(padrao());
        // sem o CPF aceito, volta ao login
        let r = pedir(&p, "GET", &format!("{REALM}/validacao"), "", "");
        assert_eq!(r.local().unwrap(), format!("{REALM}/login"));
        let r = pedir(&p, "GET", &format!("{REALM}/validacao"), "", "cpf=ok");
        let h = r.texto();
        assert!(h.contains("Escolha o email ou telefone para receber o código de validação"));
        assert!(h.contains(&format!(
            "<input type=\"radio\" name=\"mail\" id=\"{EMAIL_MASCARADO}\" checked>"
        )));
        assert!(h.contains(&format!(
            "<input type=\"radio\" name=\"tel\" id=\"{TELEFONE_MASCARADO}\">"
        )));
        assert!(h.contains("<button type=\"submit\" name=\"login\">Receber código</button>"));
        assert!(h.contains("Quero alterar meus dados"));
        // o e-mail inteiro nunca aparece (nem aqui nem em id algum)
        assert!(!h.contains("fulano@"));
        // só `button[name="login"]` é o de Receber código: o outro tem outro name
        assert_eq!(h.matches("name=\"login\"").count(), 1);
    }

    #[test]
    fn receber_codigo_gera_seis_digitos_e_o_email_so_com_o_radio_de_email() {
        let p = portal(padrao());
        assert!(p.codigo_gerado().is_none());
        assert!(p.email_do_codigo().is_none());
        // radio de SMS: nada na caixa de e-mail
        let r = pedir(
            &p,
            "POST",
            &format!("{REALM}/validacao"),
            "tel=on&login=",
            "cpf=ok",
        );
        assert_eq!(r.local().unwrap(), format!("{REALM}/validacao"));
        assert!(p.codigo_gerado().is_none());
        // radio de e-mail
        let r = pedir(
            &p,
            "POST",
            &format!("{REALM}/validacao"),
            "mail=on&login=",
            "cpf=ok",
        );
        assert_eq!(r.local().unwrap(), format!("{REALM}/codigo"));
        let codigo = p.codigo_gerado().unwrap();
        assert_eq!(codigo.len(), 6);
        assert!(codigo.chars().all(|c| c.is_ascii_digit()));
        let (corpo, quando) = p.email_do_codigo().unwrap();
        assert!(
            corpo.starts_with(&format!("Código de ativação: {codigo}")),
            "{corpo}"
        );
        assert_eq!(Some(quando), p.instante_do_codigo());
    }

    #[test]
    fn codigo_errado_volta_ao_codigo_e_conta() {
        let p = portal(padrao());
        pedir(
            &p,
            "POST",
            &format!("{REALM}/validacao"),
            "mail=on",
            "cpf=ok",
        );
        let certo = p.codigo_gerado().unwrap();
        let errado = if certo == "000000" {
            "111111"
        } else {
            "000000"
        };
        let r = pedir(
            &p,
            "POST",
            &format!("{REALM}/codigo"),
            &format!("codigo={errado}"),
            "cpf=ok",
        );
        assert_eq!(r.local().unwrap(), format!("{REALM}/codigo?erro=1"));
        assert_eq!(p.codigos_recusados(), 1);
        let r = pedir(&p, "GET", &format!("{REALM}/codigo?erro=1"), "", "cpf=ok");
        assert!(r.texto().contains("id=\"codigo\""));
        assert!(
            r.texto()
                .contains("Por favor, insira o código de validação recebido")
        );
        // sem o código aceito, a senha não abre
        let r = pedir(&p, "GET", &format!("{REALM}/senha"), "", "cpf=ok");
        assert_eq!(r.local().unwrap(), format!("{REALM}/login"));
    }

    #[test]
    fn a_pagina_da_senha_tem_a_estrutura_do_portal() {
        let p = portal(padrao());
        let r = pedir(
            &p,
            "GET",
            &format!("{REALM}/senha"),
            "",
            "cpf=ok; codigo=ok",
        );
        let h = r.texto();
        assert!(h.contains("Informe sua senha:"));
        assert!(h.contains("id=\"template-section\""));
        assert!(h.contains("<input id=\"password\" name=\"password\" type=\"password\""));
        assert_eq!(h.matches("<form").count(), 2, "o 1º form entra, o 2º volta");
        // o botão Entrar é o 1º `button` da 1ª `div` do 1º `form`
        let depois_do_form = &h[h.find("<form").unwrap()..];
        let primeiro_form = &depois_do_form[..depois_do_form.find("</form>").unwrap()];
        assert!(primeiro_form.contains("<div><label"));
        assert!(primeiro_form.contains("<button type=\"submit\">Entrar</button></div>"));
        assert!(h.contains("<button id=\"kc-cancel\""));
        assert!(h.contains("Recuperar Senha"));
    }

    #[test]
    fn senha_errada_volta_a_senha_com_o_campo_e_conta_login_recusado() {
        let p = portal(padrao());
        let r = pedir(
            &p,
            "POST",
            &format!("{REALM}/senha"),
            "password=errada",
            "cpf=ok; codigo=ok",
        );
        assert_eq!(r.local().unwrap(), format!("{REALM}/senha?erro=1"));
        assert!(r.cookie().is_none());
        assert_eq!(p.logins_recusados(), 1);
        assert_eq!(p.logins(), 0);
        let r = pedir(
            &p,
            "GET",
            &format!("{REALM}/senha?erro=1"),
            "",
            "cpf=ok; codigo=ok",
        );
        assert!(r.texto().contains("id=\"password\""));
        assert!(r.texto().contains("Senha inválida"));
    }

    #[test]
    fn rejeitar_senha_recusa_ate_a_certa() {
        let p = portal(padrao().com_rejeitar_senha(true));
        let r = pedir(
            &p,
            "POST",
            &format!("{REALM}/senha"),
            "password=SenhaFicticia%234821",
            "cpf=ok; codigo=ok",
        );
        assert_eq!(r.local().unwrap(), format!("{REALM}/senha?erro=1"));
        assert_eq!(p.logins(), 0);
        assert_eq!(p.logins_recusados(), 1);
    }

    #[test]
    fn o_login_inteiro_abre_a_home_com_app_home() {
        let p = portal(padrao());
        let cookies = entrar(&p);
        assert_eq!(p.logins(), 1);
        assert_eq!(p.logins_recusados(), 0);
        assert_eq!(p.codigos_recusados(), 0);
        let r = pedir(&p, "GET", "/home", "", &cookies);
        let h = r.texto();
        assert!(h.contains("<app-home>"));
        assert!(h.contains("Ir para Contracheque"));
        assert!(
            h.contains("Ocorreu um erro inesperado"),
            "o aviso transitório"
        );
        assert!(h.contains("a.remove()"), "e ele some sozinho");
        // sem sessão, a home leva ao login
        let r = pedir(&p, "GET", "/home", "", "");
        assert_eq!(r.local().unwrap(), format!("{REALM}/login"));
        // e o login, com sessão, leva à home
        let r = pedir(&p, "GET", &format!("{REALM}/login"), "", &cookies);
        assert_eq!(r.local().unwrap(), "/home");
    }

    #[test]
    fn a_pagina_do_contracheque_tem_calendario_com_meses_desabilitados_e_modelos() {
        let p = portal(padrao());
        let cookies = entrar(&p);
        // sem sessão, leva ao login
        let r = pedir(&p, "GET", "/contraCheque/11/2026", "", "");
        assert_eq!(r.local().unwrap(), format!("{REALM}/login"));
        let r = pedir(&p, "GET", "/contraCheque/10/2026", "", &cookies);
        assert_eq!(r.status, 200);
        let h = r.texto();
        assert!(h.contains("id=\"matFormFieldDataTemplate\""));
        assert!(h.contains(
            "<mat-datepicker-toggle><button type=\"button\" aria-label=\"Open calendar\">"
        ));
        assert!(h.contains("<mat-datepicker id=\"mat-datepicker-0\" hidden>"));
        assert!(
            h.contains("<mat-calendar-header><button type=\"button\" aria-label=\"Choose date\">")
        );
        assert!(h.contains("aria-label=\"Previous year\""));
        assert!(h.contains("aria-label=\"Next year\""));
        assert!(h.contains("<mat-year-view></mat-year-view>"));
        // a regra dos meses desabilitados vai no script (corrente e mínimo do portal)
        assert!(h.contains("var CORRENTE='2026-11',MINIMO='2026-05';"));
        assert!(h.contains("k<MINIMO||k>CORRENTE"));
        // um modelo por mês publicado; o não publicado não tem
        assert!(h.contains("<template id=\"m-2026-10\">"));
        assert!(h.contains("<template id=\"m-2026-11\">"));
        assert!(!h.contains("<template id=\"m-2026-09\">"));
        assert!(h.contains("<h2>Outubro 2026</h2>"));
        // a rota não troca o mês: a página sempre abre no corrente
        assert!(h.contains("escolher(CORRENTE)"));
        // nenhuma `#interna` pronta: ela só nasce depois de escolher o mês
        assert!(!h.contains("id=\"interna\""));
    }

    /// O caminho posicional (`div:nth-of-type`) do botão de data-acao dentro do cartão do mês:
    /// os índices de cada `div` ancestral entre os `div` irmãos, e o do botão entre os `button`.
    fn caminho_do_botao(html: &str, acao: &str) -> Option<(Vec<usize>, usize)> {
        let mut pilha: Vec<usize> = vec![0]; // contador de `div` filhos de cada nível aberto
        let mut caminho: Vec<usize> = Vec::new();
        let mut botoes: Vec<usize> = vec![0];
        let mut resto = html;
        while let Some(i) = resto.find('<') {
            resto = &resto[i..];
            let fim = resto.find('>')?;
            let tag = &resto[..=fim];
            if tag.starts_with("<div") {
                *pilha.last_mut()? += 1;
                caminho.push(*pilha.last()?);
                pilha.push(0);
                botoes.push(0);
            } else if tag == "</div>" {
                pilha.pop();
                botoes.pop();
                caminho.pop();
            } else if tag.starts_with("<button") {
                *botoes.last_mut()? += 1;
                if tag.contains(&format!("data-acao=\"{acao}\"")) {
                    return Some((caminho.clone(), *botoes.last()?));
                }
            }
            resto = &resto[fim + 1..];
        }
        None
    }

    #[test]
    fn o_baixar_pdf_esta_no_caminho_posicional_do_portal_real() {
        // #interna > div:nth-of-type(1) > div:nth-of-type(3) > div:nth-of-type(8)
        //   > div:nth-of-type(2) > button:nth-of-type(1)
        let cartao = cartao_do_mes("2026-10");
        let (caminho, botao) = caminho_do_botao(&cartao, "baixar").unwrap();
        assert_eq!(caminho, vec![1, 3, 8, 2], "{cartao}");
        assert_eq!(botao, 1);
        // o Voltar é o 1º `div` do mesmo rodapé
        let (caminho, botao) = caminho_do_botao(&cartao, "voltar").unwrap();
        assert_eq!(caminho, vec![1, 3, 8, 1]);
        assert_eq!(botao, 1);
        assert!(cartao.contains("title=\"Clique aqui para baixar seu contracheque\""));
        assert!(cartao.contains("aria-label=\"Clique aqui para baixar seu contracheque\""));
        for dado in [
            "Fulano de Tal",
            "CX-0001",
            "Salário bruto",
            "Salário líquido",
        ] {
            assert!(cartao.contains(dado), "{dado}");
        }
    }

    #[test]
    fn baixar_serve_o_pdf_com_o_nome_do_portal_e_conta_por_mes() {
        let p = portal(padrao());
        let cookies = entrar(&p);
        // sem sessão, não baixa
        let r = pedir(&p, "GET", "/api/contraCheque/2026-11.pdf", "", "");
        assert_eq!(r.local().unwrap(), format!("{REALM}/login"));
        assert_eq!(p.downloads(), 0);
        let r = pedir(&p, "GET", "/api/contraCheque/2026-11.pdf", "", &cookies);
        assert_eq!(r.status, 200);
        assert_eq!(r.cabecalho_de("content-type").unwrap(), "application/pdf");
        assert_eq!(
            r.cabecalho_de("content-disposition").unwrap(),
            "attachment; filename=\"ContraCheque_11_2026.pdf\""
        );
        assert_eq!(r.corpo, pdf_do_contracheque("2026-11", None));
        assert!(p.baixou("2026-11"));
        assert!(!p.baixou("2026-10"));
        pedir(&p, "GET", "/api/contraCheque/2026-11.pdf", "", &cookies);
        assert_eq!(p.baixadas_de("2026-11"), 2);
        assert_eq!(p.downloads(), 2);
        // mês não publicado ou inventado: 404 e nada contado
        assert_eq!(
            pedir(&p, "GET", "/api/contraCheque/2026-09.pdf", "", &cookies).status,
            404
        );
        assert_eq!(
            pedir(&p, "GET", "/api/contraCheque/lixo.pdf", "", &cookies).status,
            404
        );
        assert_eq!(p.downloads(), 2);
    }

    #[test]
    fn dois_downloads_sao_iguais_ou_mudam_so_os_bytes_conforme_a_config() {
        let fixo = portal(padrao());
        let c = entrar(&fixo);
        let a = pedir(&fixo, "GET", "/api/contraCheque/2026-11.pdf", "", &c).corpo;
        let b = pedir(&fixo, "GET", "/api/contraCheque/2026-11.pdf", "", &c).corpo;
        assert_eq!(a, b);

        let novo = portal(padrao().com_pdf_bytes_novos_a_cada_baixa(true));
        let c = entrar(&novo);
        let a = pedir(&novo, "GET", "/api/contraCheque/2026-11.pdf", "", &c).corpo;
        let b = pedir(&novo, "GET", "/api/contraCheque/2026-11.pdf", "", &c).corpo;
        assert_ne!(a, b, "os bytes mudam a cada geração");
        // o texto da página (o fluxo de conteúdo) é o mesmo nos dois
        let fluxo = |pdf: &[u8]| {
            let t = String::from_utf8_lossy(pdf).into_owned();
            let i = t.find("stream\n").unwrap();
            let j = t.find("endstream").unwrap();
            t[i..j].to_owned()
        };
        assert_eq!(fluxo(&a), fluxo(&b));
    }

    #[test]
    fn o_pdf_e_valido_tem_texto_do_mes_e_ao_menos_600_caracteres() {
        let pdf = pdf_do_contracheque("2026-10", None);
        let t = String::from_utf8(pdf.clone()).unwrap();
        assert!(t.starts_with("%PDF-1.4\n"));
        assert!(t.ends_with("%%EOF\n"));
        // o `startxref` aponta para a palavra `xref`
        let inicio: usize = t
            .split("startxref\n")
            .nth(1)
            .unwrap()
            .lines()
            .next()
            .unwrap()
            .parse()
            .unwrap();
        assert!(t[inicio..].starts_with("xref\n"));
        // cada entrada da `xref` aponta para `N 0 obj`
        let tabela = &t[inicio..];
        for (i, linha) in tabela.lines().skip(3).take(6).enumerate() {
            let pos: usize = linha[..10].parse().unwrap();
            assert!(t[pos..].starts_with(&format!("{} 0 obj", i + 1)), "{linha}");
        }
        let texto = linhas_do_contracheque("2026-10").join("\n");
        assert!(texto.chars().count() >= 600, "{}", texto.chars().count());
        assert!(texto.contains("Outubro 2026"));
        assert!(t.contains("CONTRACHEQUE FICT\\315CIO - Outubro 2026"));
        // o arquivo inteiro é ASCII (acentos como escape octal)
        assert!(pdf.is_ascii());
        // o texto de um mês nunca é o de outro
        assert_ne!(
            linhas_do_contracheque("2026-10"),
            linhas_do_contracheque("2026-11")
        );
    }

    #[test]
    fn o_pdf_e_legivel_pelo_pdftotext_quando_ele_existe() {
        if std::process::Command::new("pdftotext")
            .arg("-v")
            .output()
            .is_err()
        {
            eprintln!("pulado: sem pdftotext");
            return;
        }
        let pasta = std::env::temp_dir().join(format!("sbf-soucaixa-pdf-{}", std::process::id()));
        std::fs::create_dir_all(&pasta).unwrap();
        let arquivo = pasta.join("c.pdf");
        std::fs::write(&arquivo, pdf_do_contracheque("2026-03", Some("g1"))).unwrap();
        let saida = std::process::Command::new("pdftotext")
            .arg(&arquivo)
            .arg("-")
            .output()
            .unwrap();
        let _ = std::fs::remove_dir_all(&pasta);
        let texto = String::from_utf8_lossy(&saida.stdout).into_owned();
        assert!(texto.contains("Março 2026"), "{texto}");
        assert!(texto.contains("Fulano de Tal"), "{texto}");
        assert!(
            texto.chars().filter(|c| !c.is_whitespace()).count() >= 400,
            "{texto}"
        );
    }

    #[test]
    fn os_acessos_proibidos_sao_contados() {
        let p = portal(padrao());
        assert!(p.acessos_proibidos().is_empty());
        for (m, r) in [
            ("POST", format!("{REALM}/alterar-dados")),
            ("POST", format!("{REALM}/reenviar")),
            ("POST", format!("{REALM}/cancelar")),
            ("GET", format!("{REALM}/recuperar")),
            ("GET", "/pedidos".to_owned()),
            ("GET", "/adiantamentos".to_owned()),
            ("GET", "/auxilios".to_owned()),
            ("GET", "/perfil".to_owned()),
            ("GET", "/sair".to_owned()),
            ("POST", "/api/voltar".to_owned()),
        ] {
            pedir(&p, m, &r, "", "");
        }
        assert_eq!(p.acessos_proibidos().len(), 10);
        assert!(
            p.acessos_proibidos()
                .contains(&"POST /api/voltar".to_owned())
        );
        // nada disso conta como login nem como download
        assert_eq!((p.logins(), p.downloads()), (0, 0));
    }

    #[test]
    fn titulo_do_mes_e_a_inicial_maiuscula_com_o_ano() {
        assert_eq!(titulo_do_mes("2026-10"), "Outubro 2026");
        assert_eq!(titulo_do_mes("2026-03"), "Março 2026");
        assert_eq!(titulo_do_mes("lixo"), "lixo");
    }

    #[test]
    fn dinheiro_formata_em_reais() {
        assert_eq!(dinheiro(500_000), "R$ 5.000,00");
        assert_eq!(dinheiro(12_345_678), "R$ 123.456,78");
        assert_eq!(dinheiro(5), "R$ 0,05");
    }

    #[test]
    fn meses_publicados_invalidos_sao_ignorados_e_o_corrente_ruim_cai_no_minimo() {
        let p = portal(
            Config::nova("1", "x", "lixo").com_meses_publicados(&["2026-13", "2026-05", "abc"]),
        );
        let c = entrar_com(&p);
        let h = pedir(&p, "GET", "/contraCheque/05/2026", "", &c).texto();
        assert!(h.contains("<template id=\"m-2026-05\">"));
        assert!(!h.contains("m-2026-13"));
        assert!(h.contains(&format!("CORRENTE='{PRIMEIRO_MES_HABILITADO}'")));
    }

    /// Como [`entrar`], para um portal com CPF e senha de outros valores (só o cookie importa).
    fn entrar_com(p: &PortalDeSouCaixa) -> String {
        let r = pedir(p, "POST", &format!("{REALM}/login"), "username=1", "");
        assert_eq!(r.cookie().as_deref(), Some("cpf=ok"));
        pedir(
            p,
            "POST",
            &format!("{REALM}/validacao"),
            "mail=on",
            "cpf=ok",
        );
        let codigo = p.codigo_gerado().unwrap();
        pedir(
            p,
            "POST",
            &format!("{REALM}/codigo"),
            &format!("codigo={codigo}"),
            "cpf=ok",
        );
        let r = pedir(
            p,
            "POST",
            &format!("{REALM}/senha"),
            "password=x",
            "cpf=ok; codigo=ok",
        );
        assert_eq!(r.cookie().as_deref(), Some("sessao=ok"));
        "cpf=ok; codigo=ok; sessao=ok".to_owned()
    }
}
