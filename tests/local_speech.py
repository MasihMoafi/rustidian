"""Pronunciation regressions; run with the installed local voice environment."""
from pathlib import Path
import sys
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "assets/read-aloud"))
from speech_text import phonemize, prepare, pronunciation


class PronunciationTests(unittest.TestCase):
    def test_abbreviations_in_a_sentence_are_spoken_as_words(self):
        self.assertEqual(
            phonemize("Use a database, e.g. PostgreSQL; i.e. keep the state in one place."),
            phonemize("Use a database, for example PostgreSQL; that is keep the state in one place."),
        )
        self.assertEqual(prepare("E.G. an example; I.E. an explanation."),
                         "for example an example; that is an explanation.")
        self.assertEqual(prepare("Keep some.e.g.identifier and 1.5 unchanged."),
                         "Keep some.e.g.identifier and 1.5 unchanged.")

    def test_live_and_lives_keep_their_verb_and_other_senses(self):
        for text, sound in (
            ("I live in Tehran.", "lˈɪv"),
            ("This is a live broadcast.", "lˈIv"),
            ("The database is where the state lives.", "lˈɪvz"),
            ("A cat has nine lives.", "lˈIvz"),
        ):
            with self.subTest(text=text):
                self.assertIn(sound, phonemize(text))

    def test_other_heteronyms_use_sentence_context(self):
        for text, sound in (
            ("Please read this article.", "ɹˈid"),
            ("She read it yesterday.", "ɹˈɛd"),
            ("The wind blew.", "wˈɪnd"),
            ("Please wind the clock.", "wˈInd"),
        ):
            with self.subTest(text=text):
                self.assertIn(sound, phonemize(text))

    def test_missing_language_model_never_triggers_an_implicit_download(self):
        pronunciation.cache_clear()
        try:
            with patch("spacy.util.is_package", return_value=False), patch(
                "spacy.cli.download", side_effect=AssertionError("Implicit network download")
            ):
                with self.assertRaisesRegex(RuntimeError, "English pronunciation model"):
                    phonemize("Read offline.")
        finally:
            pronunciation.cache_clear()


if __name__ == "__main__":
    unittest.main()
