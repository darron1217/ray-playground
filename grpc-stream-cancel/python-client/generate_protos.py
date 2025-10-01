import grpc_tools.protoc

grpc_tools.protoc.main([
    'grpc_tools.protoc',
    '-I../proto',
    '--python_out=.',
    '--grpc_python_out=.',
    '../proto/streaming.proto'
])