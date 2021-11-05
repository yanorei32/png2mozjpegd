@echo off

image-compress-daemon config.vrchat.yml

if not %ERRORLEVEL% == 0 (
    echo
    echo ---------------------------
    echo + Exit Code: %ERRORLEVEL%
    pause
)
