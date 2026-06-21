package com.nalomu.nalu

import android.Manifest
import android.content.Intent
import android.os.Build
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.compose.setContent
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.runtime.LaunchedEffect
import androidx.lifecycle.viewmodel.compose.viewModel
import com.nalomu.nalu.ui.NaluMobileRoot
import com.nalomu.nalu.ui.NaluTheme
import com.nalomu.nalu.ui.NaluViewModel

class MainActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        val sharedText = extractSharedText(intent)
        val container = (application as NaluApp).container

        setContent {
            val notificationPermissionLauncher = rememberLauncherForActivityResult(
                ActivityResultContracts.RequestPermission()
            ) {}
            LaunchedEffect(Unit) {
                if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
                    notificationPermissionLauncher.launch(Manifest.permission.POST_NOTIFICATIONS)
                }
            }

            val viewModel: NaluViewModel = viewModel(
                factory = NaluViewModel.Factory(container.repository, container.settingsStore)
            )
            LaunchedEffect(sharedText) {
                if (!sharedText.isNullOrBlank()) {
                    viewModel.addNote("分享文本", sharedText)
                }
            }
            NaluTheme {
                NaluMobileRoot(viewModel = viewModel)
            }
        }
    }

    private fun extractSharedText(intent: Intent?): String? {
        if (intent?.action != Intent.ACTION_SEND || intent.type != "text/plain") return null
        return intent.getStringExtra(Intent.EXTRA_TEXT)
    }
}
