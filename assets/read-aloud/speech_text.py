"""Prepare written English for the local speech engines without changing the note."""
from functools import cache
import re


def prepare(text):
    for pattern, replacement in (
        (r"(?<![\w.])e\.\s*g\.(?!\w)", "for example"),
        (r"(?<![\w.])i\.\s*e\.(?!\w)", "that is"),
    ):
        text = re.sub(pattern, replacement, text, flags=re.IGNORECASE)
    return text


@cache
def pronunciation():
    import spacy.util
    from misaki import en, espeak

    # Misaki otherwise tries to install the language model during a read. Setup owns
    # downloads; reading an article must work offline and never download implicitly.
    if not spacy.util.is_package("en_core_web_sm"):
        raise RuntimeError("Local reading needs the English pronunciation model. Run voice setup.")
    return en.G2P(
        trf=False,
        british=False,
        fallback=espeak.EspeakFallback(british=False),
    )


def phonemize(text):
    phonemes, _ = pronunciation()(prepare(text))
    return phonemes
