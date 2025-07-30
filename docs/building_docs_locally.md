# Test-building docs locally

While writing, you can build and view docs locally for fast iterating. Locally built docs (at least HTML)
look and behave very similar to how they will look once at RTD.

(Optional) If needed, create a separate environment to install `sphinx` and dependencies:
```
conda create -n ni-docs python=3.12
conda activate ni-docs
```

Install the necessary Python dependencies:
```
cd ni-streamer/docs
pip install -r requirements.txt
```

Use the Makefile to build the docs, like so:
```
make builder_name
```
where "builder_name" is one of the supported builders, e.g. `html`, `latex` or `linkcheck`. If building with `html`,
the output will be located in `/docs/_build/html`.

Use 
```
make clean
make builder_name
```
to re-build from scratch.
