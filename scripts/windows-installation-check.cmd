@echo off
setlocal DisableDelayedExpansion
"%SystemRoot%\System32\chcp.com" 65001 >nul
set "OPENNOVEL_CHECK_EXE=%~dp0opennovel-desktop.exe"
if not exist "%OPENNOVEL_CHECK_EXE%" set "OPENNOVEL_CHECK_EXE=%~dp0OpenNovel.exe"
if not exist "%OPENNOVEL_CHECK_EXE%" goto missing
set "OPENNOVEL_CHECK_OUTPUT=%LOCALAPPDATA%\OpenNovel-Installation-Checks"
if not "%~1"=="" set "OPENNOVEL_CHECK_OUTPUT=%~f1"
echo 正在检查 OpenNovel 安装，约需数秒。
echo 本次只使用新建的原创样本，不打开你的书库，也不请求小说平台。
start "" /wait "%OPENNOVEL_CHECK_EXE%" --check-installation "%OPENNOVEL_CHECK_OUTPUT%"
set "OPENNOVEL_CHECK_CODE=%ERRORLEVEL%"
if not "%OPENNOVEL_CHECK_CODE%"=="0" goto failed
echo.
echo 基础自检通过。可选番茄组件的状态请查看报告。
echo 这不代表账号、全书下载或实际窗口操作已经验收。
goto finish
:missing
set "OPENNOVEL_CHECK_CODE=2"
echo 未找到 OpenNovel 主程序。请将本文件放在安装目录中，再双击运行。
goto finish
:failed
echo.
echo 基础自检未通过，错误码：%OPENNOVEL_CHECK_CODE%
echo 请保留新生成的报告，按 README.txt 中的提示检查。
:finish
if defined OPENNOVEL_CHECK_OUTPUT echo 报告位置："%OPENNOVEL_CHECK_OUTPUT%"
if not "%~2"=="--quiet" pause
exit /b %OPENNOVEL_CHECK_CODE%
