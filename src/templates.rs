use {super::*, boilerplate::Boilerplate};

pub(crate) use {
  crate::subcommand::server::ServerConfig,
  address::AddressHtml,
  block::BlockHtml,
  children::ChildrenHtml,
  clock::ClockSvg,
  collections::CollectionsHtml,
  home::HomeHtml,
  iframe::Iframe,
  input::InputHtml,
  inscriptions::InscriptionsHtml,
  inscriptions_block::InscriptionsBlockHtml,
  metadata::MetadataHtml,
  output::OutputHtml,
  parents::ParentsHtml,
  preview::{
    PreviewAudioHtml, PreviewCodeHtml, PreviewFontHtml, PreviewImageHtml, PreviewMarkdownHtml,
    PreviewModelHtml, PreviewPdfHtml, PreviewTextHtml, PreviewUnknownHtml, PreviewVideoHtml,
  },
  rare::RareTxt,
  sat::SatHtml,
};

pub use {
  blocks::BlocksHtml, inscription::InscriptionHtml, relic::RelicHtml,
  relic_events::RelicEventsHtml, relics::RelicsHtml, rune::RuneHtml, runes::RunesHtml,
  sealing::SealingHtml, sealings::SealingsHtml, status::StatusHtml, transaction::TransactionHtml,
};

pub mod address;
pub mod block;
pub mod blocks;
mod children;
mod clock;
pub mod collections;
mod home;
mod iframe;
mod input;
pub mod inscription;
pub mod inscriptions;
mod inscriptions_block;
mod metadata;
pub mod output;
mod parents;
mod preview;
mod rare;
pub mod relic;
pub mod relic_events;
pub mod relics;
pub mod rune;
pub mod runes;
pub mod sat;
pub mod sealing;
pub mod sealings;
pub mod status;
pub mod transaction;

#[derive(Boilerplate)]
pub struct PageHtml<T: PageContent> {
  content: T,
  config: Arc<ServerConfig>,
}

impl<T> PageHtml<T>
where
  T: PageContent,
{
  pub fn new(content: T, config: Arc<ServerConfig>) -> Self {
    Self { content, config }
  }

  fn og_image(&self) -> String {
    if let Some(domain) = &self.config.domain {
      format!("https://{domain}/static/og-image.jpg")
    } else {
      "https://ordinals.com/static/favicon.png".into()
    }
  }

  fn superscript(&self) -> String {
    if self.config.chain == Chain::Mainnet {
      "experimental".into()
    } else {
      self.config.chain.to_string()
    }
  }
}

pub trait PageContent: Display + 'static {
  fn title(&self) -> String;

  fn page(self, server_config: Arc<ServerConfig>) -> PageHtml<Self>
  where
    Self: Sized,
  {
    PageHtml::new(self, server_config)
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  struct Foo;

  impl Display for Foo {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
      write!(f, "<h1>Foo</h1>")
    }
  }

  impl PageContent for Foo {
    fn title(&self) -> String {
      "Foo".to_string()
    }
  }

  #[test]
  fn page() {
    assert_regex_match!(
      Foo.page(Arc::new(ServerConfig {
        chain: Chain::Mainnet,
        csp_origin: Some("https://signet.ordinals.com".into()),
        domain: Some("signet.ordinals.com".into()),
        index_sats: true,
        ..default()
      }),),
      r"<!doctype html>
<html lang=en>
  <head>
    <meta charset=utf-8>
    <meta name=format-detection content='telephone=no'>
    <meta name=viewport content='width=device-width,initial-scale=1.0'>
    <meta property=og:title content='Foo'>
    <meta property=og:image content='https://signet.ordinals.com/static/og-image.jpg'>
    <meta property=twitter:card content=summary>
    <title>Foo</title>
    <link rel=alternate href=/feed.xml type=application/rss\+xml title='Inscription Feed'>
    <link rel=icon href=/static/favicon.png>
    <link rel=icon href=/static/favicon.svg>
    <link rel=stylesheet href=/static/index.css>
    <link rel=stylesheet href=/static/modern-normalize.css>
    <script src=/static/index.js></script>
  </head>
  <body>
  <header>
    <nav>
      <a href=/ title=home>Relics<sup>experimental</sup></a>
      .*
      <a href=/clock title=clock>.*</a>
      <a href=/rare.txt title=rare>.*</a>
      .*
      <form action=/search method=get>
        <input type=text .*>
        <input class=icon type=image .*>
      </form>
    </nav>
  </header>
  <main>
<h1>Foo</h1>
  </main>
  </body>
</html>
"
    );
  }

  #[test]
  fn page_mainnet() {
    assert_regex_match!(
      Foo.page(Arc::new(ServerConfig {
        chain: Chain::Mainnet,
        csp_origin: None,
        domain: None,
        index_sats: true,
        ..default()
      })),
      r".*<nav>\s*<a href=/ title=home>Relics<sup>experimental</sup></a>.*"
    );
  }

  #[test]
  fn page_no_sat_index() {
    assert_regex_match!(
      Foo.page(Arc::new(ServerConfig {
        chain: Chain::Mainnet,
        csp_origin: None,
        domain: None,
        index_sats: false,
        ..default()
      })),
      r".*<nav>\s*<a href=/ title=home>Relics<sup>experimental</sup></a>.*<a href=/clock title=clock>.*</a>\s*<form action=/search.*",
    );
  }

  #[test]
  fn page_signet() {
    assert_regex_match!(
      Foo.page(Arc::new(ServerConfig {
        chain: Chain::Signet,
        csp_origin: None,
        domain: None,
        index_sats: true,
        ..default()
      })),
      r".*<nav>\s*<a href=/ title=home>Relics<sup>signet</sup></a>.*"
    );
  }
}
