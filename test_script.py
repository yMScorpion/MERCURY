import re

log_content = ""
with open("output.log", "r") as f:
    log_content = f.read()

# I don't have output.log from the new run. Let's just use grep on the source to see where else opps_detected is logged.
