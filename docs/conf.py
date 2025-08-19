import os
import sys
sys.path.insert(0, os.path.abspath('../py_api'))

# Configuration file for the Sphinx documentation builder.
#
# For the full list of built-in configuration values, see the documentation:
# https://www.sphinx-doc.org/en/master/usage/configuration.html

# -- Project information -----------------------------------------------------
# https://www.sphinx-doc.org/en/master/usage/configuration.html#project-information
project = 'NI Pulse Streamer'
copyright = '2025, Project contributors'
author = 'Project contributors'
release = '0.1.0'
github_url = 'https://github.com/NIStreamer/nistreamer'

# -- General configuration ---------------------------------------------------
# https://www.sphinx-doc.org/en/master/usage/configuration.html#general-configuration
extensions = [
    'myst_nb',
    'sphinx.ext.napoleon',
    'sphinx.ext.autodoc',
    'sphinx.ext.autosummary',
    'sphinx.ext.viewcode',
    'sphinx_autodoc_typehints',
]
autodoc_mock_imports = ['nistreamer._nistreamer', 'numpy', 'plotly']
autodoc_member_order = "bysource"
autosummary_generate = True
napoleon_google_docstring = True
napoleon_use_seealso = True
myst_enable_extensions = ["colon_fence"]
nb_execution_mode = "off"  # Don't execute notebooks on build
templates_path = ['_templates']
exclude_patterns = ['_build', 'Thumbs.db', '.DS_Store', 'building_docs_locally.md']

# -- Options for HTML output -------------------------------------------------
# https://www.sphinx-doc.org/en/master/usage/configuration.html#options-for-html-output
html_theme = 'pydata_sphinx_theme'
html_static_path = ['_static']
html_show_sourcelink = False
html_theme_options = {
    "logo": {
        "text": "NI Pulse Streamer",
    },
    "github_url": "https://github.com/NIStreamer/nistreamer",
    "use_edit_page_button": False,
    "show_toc_level": 2,
}
