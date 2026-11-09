"""Independent literal protocol witnesses; not substitutes for native sessions."""
import unittest
from wayland_surface import recreated_main_surfaces


TRACE = '''
wl_compositor@3.create_surface(new id wl_surface@24)
xdg_wm_base@11.get_xdg_surface(new id xdg_surface@26, wl_surface@24)
xdg_surface@26.get_toplevel(new id xdg_toplevel@27)
zxdg_exporter_v2@50.export_toplevel(new id zxdg_exported_v2@55, wl_surface@24)
zxdg_exported_v2@55.handle("hidden")
wl_compositor@3.create_surface(new id wl_surface@51)
xdg_wm_base@11.get_xdg_surface(new id xdg_surface@52, wl_surface@51)
xdg_surface@52.get_toplevel(new id xdg_toplevel@53)
zxdg_exporter_v2@50.export_toplevel(new id zxdg_exported_v2@57, wl_surface@51)
zxdg_exported_v2@57.handle("before")
zxdg_exported_v2@57.destroy()
wl_surface@24.destroy()
wl_surface@51.destroy()
wl_compositor@3.create_surface(new id wl_surface@51)
xdg_wm_base@11.get_xdg_surface(new id xdg_surface@52, wl_surface@51)
xdg_surface@52.get_toplevel(new id xdg_toplevel@53)
zxdg_exporter_v2@50.export_toplevel(new id zxdg_exported_v2@57, wl_surface@51)
zxdg_exported_v2@57.handle("after")
'''


class SurfaceLifetimeTests(unittest.TestCase):
    def test_parent_tokens_select_main_and_allow_surface_and_export_id_reuse(self):
        self.assertEqual(recreated_main_surfaces(TRACE, "wayland:before", "wayland:after"), ["51", "51"])

    def test_hidden_surface_destruction_does_not_prove_main_destruction(self):
        with self.assertRaisesRegex(RuntimeError, "main surface was not destroyed"):
            recreated_main_surfaces(TRACE.replace("wl_surface@51.destroy()", ""), "wayland:before", "wayland:after")

    def test_unrelated_new_surface_does_not_prove_main_recreation(self):
        decoy = TRACE
        # Replace only the final main creation/binding, leaving the exported parent
        # bound to51: an unrelated recreated35 must not qualify the real parent.
        boundary = decoy.index("wl_surface@51.destroy()") + len("wl_surface@51.destroy()")
        decoy = decoy[:boundary] + decoy[boundary:].replace("wl_surface@51)", "wl_surface@35)", 2)
        with self.assertRaisesRegex(RuntimeError, "new native toplevel binding"):
            recreated_main_surfaces(decoy, "wayland:before", "wayland:after")

    def test_new_binding_without_native_surface_creation_is_refused(self):
        boundary = TRACE.index("wl_surface@51.destroy()") + len("wl_surface@51.destroy()")
        malformed = TRACE[:boundary] + TRACE[boundary:].replace("wl_compositor@3.create_surface(new id wl_surface@51)", "")
        with self.assertRaisesRegex(RuntimeError, "new native surface creation"):
            recreated_main_surfaces(malformed, "wayland:before", "wayland:after")


if __name__ == "__main__":
    unittest.main()
