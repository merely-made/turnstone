"""Controls against reintroducing engines through a transitive feature edge."""
import unittest
from check_default_links import parse_tree, violations

class CompositionTests(unittest.TestCase):
    def test_parser_collects_features_and_duplicate_paths(self):
        packages = parse_tree('turnstone v0.1.0 (/checkout) [default,audio]\nredshank-playback v0.0.0 (git#abc) [native] (*)\n')
        self.assertEqual(packages['turnstone'], {'default','audio'})
        self.assertEqual(packages['redshank-playback'], {'native'})

    def test_transitive_leaks_fail_without_their_host_feature(self):
        for package in ['firewheel','knot-editor','distillery','servo','welding','retinue','mere-transport']:
            self.assertTrue(violations({'turnstone':{'default'}, package:set()}), package)
        self.assertTrue(violations({'turnstone':{'default'},'graphshell-host':{'native'}}))
        self.assertTrue(violations({'turnstone':{'default'},'mere-athanor':{'workshop-worker'},'mere-alembic':{'workshop-native'},'redshank-playback':{'native'},'graphshell-host':{'native'}}))

    def test_explicit_selection_is_a_positive_control(self):
        self.assertFalse(violations({'turnstone':{'audio','peers','authoring','workshop'},'firewheel':set(),'mere-transport':set(),'knot-editor':set(),'mere-athanor':{'workshop-worker'},'mere-alembic':{'workshop-native'},'redshank-playback':{'native'},'graphshell-host':{'native'}}))

    def test_a_selected_but_unwired_backend_fails_the_positive_control(self):
        self.assertTrue(violations({'turnstone':{'audio'}}))
        self.assertTrue(violations({'turnstone':{'workshop'},'mere-athanor':set()}))

    def test_media_player_is_shared_by_the_two_explicit_audio_owners(self):
        self.assertFalse(violations({'turnstone':{'servo'},'servo':set(),'graft-engine':set(),'servo-media-player':set()}))
        self.assertTrue(violations({'turnstone':{'default'},'servo-media-player':set()}))

    def test_empty_or_wrong_cargo_output_is_not_a_pass(self):
        with self.assertRaises(ValueError):
            parse_tree('')
