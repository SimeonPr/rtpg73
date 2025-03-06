#!/bin/bash
OUTPUT_NAME="hall_request_assigner"

dmd main.d config.d elevator_algorythm.d elevator_state.d optimal_hall_request.dd_jason/jsonx.d -w -g -of$OUTPUT_NAME

chmod +x $OUTPUT_NAME

echo "Build complete. Run ./$OUTPUT_NAME to executable."