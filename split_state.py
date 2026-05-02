import re
import os

with open("src/state.rs", "r") as f:
    content = f.read()

# I will write a simple token splitter for impl Db
