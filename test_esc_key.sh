#!/bin/bash

echo "=== ESC 키 테스트 스크립트 ==="
echo "이 스크립트는 ESC 키가 제대로 전달되는지 테스트합니다."
echo ""

echo "1. cat 명령으로 원시 키 입력 테스트:"
echo "   ESC 키를 누르면 ^[ 가 표시되어야 합니다."
echo "   (Ctrl+C로 종료)"
echo ""

echo "2. vi 모드 테스트:"
echo "   vi 에디터를 실행하고 ESC 키로 일반모드/입력모드를 전환해보세요."
echo ""

echo "테스트를 시작하려면 Enter를 누르세요..."
read

echo "=== 1. cat 테스트 시작 ==="
echo "ESC 키를 눌러보세요 (Ctrl+C로 종료):"
cat

echo ""
echo "=== 2. vi 테스트 시작 ==="  
echo "vi 에디터가 실행됩니다. ESC 키로 모드를 전환해보세요."
echo "저장하고 종료하려면: :wq"
echo ""

vi test_file.txt

echo ""
echo "=== 테스트 완료 ==="
echo "ESC 키가 제대로 작동했다면 수정이 성공한 것입니다!"
