"""Independent literal X11 parent controls; these do not replace native sessions."""
import unittest
from x11_parent import verified_portal_parents, verified_transient


CALLS = '''method call time=1 sender=:1.20 -> destination=org.freedesktop.portal.Desktop serial=1 path=/org/freedesktop/portal/desktop; interface=org.freedesktop.portal.FileChooser; member=OpenFile
   string "x11:420101"
   string "Open recipe"
   array [
      string "x11:deadbeef is merely an option, not the parent"
   ]
method call time=2 sender=:1.21 -> destination=org.freedesktop.portal.Desktop serial=1 path=/org/freedesktop/portal/desktop; interface=org.freedesktop.portal.FileChooser; member=OpenFile
   string "x11:420101"
   string "Open recipe"
method call time=3 sender=:1.22 -> destination=org.freedesktop.portal.Desktop serial=1 path=/org/freedesktop/portal/desktop; interface=org.freedesktop.portal.FileChooser; member=OpenFile
   string "x11:420101"
   string "Open recipe"
'''


class X11ParentTests(unittest.TestCase):
    def test_each_actual_request_matches_independently_observed_main_window(self):
        self.assertEqual(verified_portal_parents(CALLS, [0x420101, 0x420101, 0x420101]),
                         ["x11:420101", "x11:420101", "x11:420101"])

    def test_empty_zero_foreign_overflow_and_malformed_parent_are_refused(self):
        for parent in ["x11:", "x11:0", "x11:deadbeef", "x11:100000000", "x11:-1", "x11:xyz"]:
            with self.subTest(parent=parent), self.assertRaises(RuntimeError):
                verified_portal_parents(CALLS.replace('string "x11:420101"', 'string "' + parent + '"', 1),
                                        [0x420101, 0x420101, 0x420101])

    def test_decoy_strings_without_real_requests_do_not_qualify(self):
        with self.assertRaises(RuntimeError):
            verified_portal_parents('string "x11:420101"\n' * 3, [0x420101] * 3)

    def test_wrong_service_or_method_decoy_does_not_qualify(self):
        for decoy in [CALLS.replace("destination=org.freedesktop.portal.Desktop", "destination=com.example.Decoy"),
                      CALLS.replace("member=OpenFile", "member=SaveFile"),
                      CALLS.replace("destination=org.freedesktop.portal.Desktop", "destination=org.freedesktop.portal.Desktop.Decoy")]:
            with self.assertRaises(RuntimeError):
                verified_portal_parents(decoy, [0x420101] * 3)

    def test_valid_option_cannot_replace_a_missing_first_argument(self):
        malformed = CALLS.replace('   string "x11:420101"', '   uint32 0', 1)
        with self.assertRaisesRegex(RuntimeError, "first string parent"):
            verified_portal_parents(malformed, [0x420101] * 3)

    def test_foreign_single_request_and_extra_request_do_not_hide_in_good_calls(self):
        with self.assertRaises(RuntimeError):
            verified_portal_parents(CALLS, [0x420101, 0x420102, 0x420101])
        with self.assertRaises(RuntimeError):
            verified_portal_parents(CALLS + CALLS, [0x420101] * 3)

    def test_transient_property_is_exact_when_available(self):
        self.assertEqual(verified_transient("WM_TRANSIENT_FOR(WINDOW): window id # 0x420101\n", 0x420101), 0x420101)
        self.assertIsNone(verified_transient("WM_TRANSIENT_FOR:  not found.\n", 0x420101))
        for value in ["0x0", "0xdeadbeef", "0x100000000"]:
            with self.subTest(value=value), self.assertRaises(RuntimeError):
                verified_transient("WM_TRANSIENT_FOR(WINDOW): window id # " + value, 0x420101)

    def test_unrecognized_property_decoy_and_zero_expected_window_are_refused(self):
        with self.assertRaises(RuntimeError):
            verified_transient("WM_HINTS: window id # 0x420101", 0x420101)
        with self.assertRaises(RuntimeError):
            verified_portal_parents(CALLS, [0, 0x420101, 0x420101])


if __name__ == "__main__":
    unittest.main()
