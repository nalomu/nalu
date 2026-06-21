# Keep kotlinx serialization generated serializers for release builds.
-keepclassmembers class **$$serializer { *; }
-keepclassmembers class **$Companion { *; }
