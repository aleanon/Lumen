cmake_minimum_required(VERSION 3.16)
project(bench5_qt LANGUAGES CXX)
set(CMAKE_CXX_STANDARD 17)
set(CMAKE_BUILD_TYPE RelWithDebInfo)   # -O2 plus symbols, so perf can attribute
find_package(Qt6 REQUIRED COMPONENTS Widgets)
qt_standard_project_setup()
qt_add_executable(bench5_qt qt.cpp)
target_link_libraries(bench5_qt PRIVATE Qt6::Widgets)
